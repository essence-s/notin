use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Local;

use crate::DictadoWindow;
use crate::audio;
use crate::engine;
use crate::recorder::Recorder;
use crate::resample::Resampler;
use crate::store::{append_note, move_wav_to_audio_folder};
use crate::vad::{SilenceWatcher, VadDetector};

/// Silencio que cierra la locución. Configurable en la historia #6.
const SILENCE_MS: u64 = 1_200;
/// Trozo que se le da al VAD: sólo cambia la granularidad del corte.
const VAD_CHUNK_MS: u64 = 30;
/// Rate al que opera el VAD Silero y el motor (16 kHz).
const TARGET_RATE: u32 = 16_000;

/// Una sesión de grabación en vivo: mientras existe, el micro alimenta el
/// sumidero. El `stop` es de esta sesión: el watcher lo mira para no seguir
/// vivo cuando se para a mano.
struct Session {
    capture: audio::Capture,
    samples: Arc<Mutex<Vec<i16>>>,
    stop: Arc<AtomicBool>,
}

/// La locución en curso: botón, watcher y guardado viven aquí, no en la UI.
pub struct Dictation {
    session: Arc<Mutex<Option<Session>>>,
    documents_dir: PathBuf,
    weak: slint::Weak<DictadoWindow>,
}

impl Dictation {
    pub fn new(documents_dir: PathBuf, weak: slint::Weak<DictadoWindow>) -> Self {
        Dictation {
            session: Arc::new(Mutex::new(None)),
            documents_dir,
            weak,
        }
    }

    /// Alterna entre grabar y parar, venga del botón o de un futuro atajo.
    pub fn toggle(&self) {
        let was_recording = self
            .weak
            .upgrade()
            .map(|window| window.get_recording())
            .unwrap_or(false);
        let ongoing = self.session.lock().unwrap().take();

        match ongoing {
            Some(session) => finish_session(session, &self.documents_dir, &self.weak),
            None if was_recording => {}
            None => match self.start() {
                Ok(()) => set_recording(&self.weak, true),
                Err(err) => eprintln!("no se pudo grabar: {err}"),
            },
        }
    }

    /// Arranca el micro y el watcher de silencio de una locución nueva.
    fn start(&self) -> Result<(), audio::CaptureError> {
        let samples = Arc::new(Mutex::new(Vec::new()));
        let capture = audio::start_capture(Arc::clone(&samples))?;
        let sample_rate = capture.sample_rate();
        let stop = Arc::new(AtomicBool::new(false));

        *self.session.lock().unwrap() = Some(Session {
            capture,
            samples: Arc::clone(&samples),
            stop: Arc::clone(&stop),
        });

        spawn_silence_watcher(
            samples,
            sample_rate,
            stop,
            Arc::clone(&self.session),
            self.documents_dir.clone(),
            self.weak.clone(),
        );

        Ok(())
    }
}

/// Vigila el audio nuevo sin quitarlo del sumidero: la locución completa se
/// guarda al parar. Sólo cuenta silencio después de haber oído voz.
/// El audio del micrófono (cualquier rate) se resamplea a 16 kHz antes de
/// alimentar el VAD Silero.
fn spawn_silence_watcher(
    samples: Arc<Mutex<Vec<i16>>>,
    sample_rate: u32,
    stop: Arc<AtomicBool>,
    session: Arc<Mutex<Option<Session>>>,
    documents_dir: PathBuf,
    weak: slint::Weak<DictadoWindow>,
) {
    let silence_limit = (TARGET_RATE as u64 * SILENCE_MS / 1_000) as usize;
    let window_samples = ((TARGET_RATE as u64 * VAD_CHUNK_MS / 1_000) as usize).max(1);
    let silero_path = documents_dir.join("models/silero_vad/silero_vad.onnx");

    thread::spawn(move || {
        let mut watcher = SilenceWatcher::new(silence_limit);
        let mut processed = 0usize;
        let mut resampler = match Resampler::new(sample_rate, TARGET_RATE) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("no se pudo crear el resampleador: {e}");
                return;
            }
        };
        let mut vad = match VadDetector::new(&silero_path) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("no se pudo crear el VAD Silero: {e}");
                return;
            }
        };

        loop {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
            if stop.load(Ordering::SeqCst) {
                return;
            }

            let pending_i16 = {
                let guard = samples.lock().unwrap();
                if guard.len() <= processed {
                    continue;
                }
                let pending = guard[processed..].to_vec();
                processed = guard.len();
                pending
            };

            let pending_f32: Vec<f32> = pending_i16
                .iter()
                .map(|&s| s as f32 / i16::MAX as f32)
                .collect();

            let resampled = match resampler.process(&pending_f32) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error de resampleo: {e}");
                    return;
                }
            };

            for piece in resampled.chunks(window_samples) {
                let has_speech = vad.is_speech(piece);
                watcher.feed(has_speech, piece.len());
                if watcher.should_stop() {
                    let stopped = session.lock().unwrap().take();
                    if let Some(session) = stopped {
                        finish_session(session, &documents_dir, &weak);
                    }
                    return;
                }
            }
        }
    });
}

/// Único camino de parada, venga del botón o del silencio: WAV → audio/ →
/// Markdown del día → ventana en reposo.
fn finish_session(session: Session, documents_dir: &Path, weak: &slint::Weak<DictadoWindow>) {
    session.stop.store(true, Ordering::SeqCst);

    let sample_rate = session.capture.sample_rate();
    drop(session.capture);

    let recorded = session.samples.lock().unwrap().clone();
    if let Err(err) = save_locution(&recorded, sample_rate, documents_dir) {
        eprintln!("error al guardar la locución: {err}");
    }

    set_recording(weak, false);
}

/// Guarda una locución y la añade al Markdown del día. Devuelve el Markdown.
fn save_locution(
    samples: &[i16],
    sample_rate: u32,
    documents_dir: &Path,
) -> std::io::Result<PathBuf> {
    let mut locution = Recorder::new(documents_dir.to_path_buf(), sample_rate);
    locution.record(samples);
    let wav_path = locution.stop()?;

    let now = Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let time = now.format("%H:%M:%S").to_string();
    let audio_rel = move_wav_to_audio_folder(documents_dir, &wav_path)?;

    let model_path = documents_dir.join("models/nemo-ctc-es/model.int8.onnx");
    let texto = resamplear_y_transcribir(samples, sample_rate, &model_path);

    append_note(documents_dir, &date, &time, &texto, &audio_rel)
}

/// Resamplea las muestras capturadas a 16 kHz y las transcribe con el motor.
fn resamplear_y_transcribir(
    samples: &[i16],
    sample_rate: u32,
    model_path: &Path,
) -> String {
    let f32: Vec<f32> = samples
        .iter()
        .map(|&s| s as f32 / i16::MAX as f32)
        .collect();

    let mut resampler = match Resampler::new(sample_rate, TARGET_RATE) {
        Ok(r) => r,
        Err(e) => return format!("[sin resampleo: {e}]"),
    };

    let resampled = match resampler.process(&f32) {
        Ok(r) => r,
        Err(e) => return format!("[sin resampleo: {e}]"),
    };

    match engine::transcribe(model_path, &resampled) {
        Ok(texto) => texto,
        Err(err) => {
            eprintln!("{err}");
            format!("[sin modelo: {err}]")
        }
    }
}

/// Cambia el estado visible de la ventana desde cualquier hilo.
fn set_recording(weak: &slint::Weak<DictadoWindow>, recording: bool) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = weak.upgrade() {
            window.set_recording(recording);
        }
    });
}

#[cfg(test)]
mod locution_tests {
    use super::save_locution;
    use crate::vad::{SilenceWatcher, is_speech};
    use std::fs;

    #[test]
    fn auto_stop_saves_complete_wav_and_one_note() {
        let dir = std::env::temp_dir().join("notin_test_auto_stop_save");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crea carpeta temporal");

        let mut samples: Vec<i16> = (0..16_320)
            .map(|i| if i % 2 == 0 { 4_000 } else { -4_000 })
            .collect();
        samples.extend(vec![0i16; 19_200]);

        let mut watcher = SilenceWatcher::new(19_200);
        let mut stopped = false;
        for piece in samples.chunks(480) {
            watcher.feed(is_speech(piece), piece.len());
            if watcher.should_stop() {
                stopped = true;
                break;
            }
        }
        assert!(stopped, "1,2 s de silencio tras la voz cortan la locución");

        let md = save_locution(&samples, 16_000, &dir).expect("guarda la locución");

        let wav = fs::read_dir(dir.join("audio"))
            .expect("lee audio/")
            .next()
            .expect("hay un wav")
            .expect("entrada válida")
            .path();
        let mut reader = hound::WavReader::open(&wav).expect("abre el WAV");
        let recorded: Vec<i16> = reader
            .samples()
            .map(|sample| sample.expect("lee muestra"))
            .collect();
        assert_eq!(recorded, samples, "el WAV tiene toda la locución");

        let content = fs::read_to_string(&md).expect("lee el md");
        assert_eq!(content.lines().count(), 1, "una sola nota");
        assert!(
            content.contains("[audio](audio/"),
            "la nota enlaza al audio: {content}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
