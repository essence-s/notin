use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Local;
use slint::ComponentHandle;

mod audio;
mod engine;
mod recorder;
mod store;
mod vad;

use recorder::Recorder;
use store::{append_note, move_wav_to_audio_folder};
use vad::{SilenceWatcher, is_speech};

/// Silencio que cierra la locución. Configurable en la historia #6.
const SILENCE_MS: u64 = 1_200;
/// Trozo que se le da al VAD: sólo cambia la granularidad del corte.
const VAD_CHUNK_MS: u64 = 30;

/// Una sesión de grabación en vivo: mientras existe, el micro alimenta el
/// sumidero. El `stop` es de esta sesión: el watcher lo mira para no seguir
/// vivo cuando se para a mano.
struct Session {
    capture: audio::Capture,
    samples: Arc<Mutex<Vec<i16>>>,
    stop: Arc<AtomicBool>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = AppWindow::new()?;
    let weak = window.as_weak();

    // Carpeta por defecto decidida: ~/Documentos/notin (en Windows, la de
    // Documentos del perfil). Sin config todavía: eso es la historia #6.
    let Some(documents_dir) = dirs::document_dir().map(|d| d.join("notin")) else {
        eprintln!("no encuentro la carpeta de Documentos de tu cuenta");
        return Err("no hay carpeta de Documentos".into());
    };

    // Arc<Mutex> para compartir la sesión entre el hilo de UI y el watcher.
    let session: Arc<Mutex<Option<Session>>> = Arc::new(Mutex::new(None));

    window.on_toggle({
        let session = Arc::clone(&session);
        let weak = weak.clone();
        let documents_dir = documents_dir.clone();

        move || {
            // Si la ventana dice "grabando", el clic es para parar. Cuando el
            // watcher acaba de cortar, el `take` puede venir vacío: no hay que
            // arrancar otra grabación sin querer.
            let was_recording = weak
                .upgrade()
                .map(|window| window.get_recording())
                .unwrap_or(false);
            let ongoing = session.lock().unwrap().take();

            match ongoing {
                Some(session) => finish_session(session, &documents_dir, &weak),
                None if was_recording => {}
                None => match start_session(&session, &documents_dir, &weak) {
                    Ok(()) => set_recording(&weak, true),
                    Err(err) => eprintln!("no se pudo grabar: {err}"),
                },
            }
        }
    });

    // Sonido y ventana: un hilo para cada cosa.
    // Toda locución nace y muere en el botón.
    // Y al pararlo, lo hablado va a un WAV, luego a audio/ y al Markdown del día.
    Ok(window.run()?)
}

/// Arranca el micro y el watcher de silencio de una locución nueva.
fn start_session(
    session: &Arc<Mutex<Option<Session>>>,
    documents_dir: &Path,
    weak: &slint::Weak<AppWindow>,
) -> Result<(), audio::CaptureError> {
    let samples = Arc::new(Mutex::new(Vec::new()));
    let capture = audio::start_capture(Arc::clone(&samples))?;
    let sample_rate = capture.sample_rate();
    let stop = Arc::new(AtomicBool::new(false));

    *session.lock().unwrap() = Some(Session {
        capture,
        samples: Arc::clone(&samples),
        stop: Arc::clone(&stop),
    });

    spawn_silence_watcher(
        samples,
        sample_rate,
        stop,
        Arc::clone(session),
        documents_dir.to_path_buf(),
        weak.clone(),
    );

    Ok(())
}

/// Vigila el audio nuevo sin quitarlo del sumidero: la locución completa se
/// guarda al parar. Sólo cuenta silencio después de haber oído voz.
fn spawn_silence_watcher(
    samples: Arc<Mutex<Vec<i16>>>,
    sample_rate: u32,
    stop: Arc<AtomicBool>,
    session: Arc<Mutex<Option<Session>>>,
    documents_dir: PathBuf,
    weak: slint::Weak<AppWindow>,
) {
    let silence_limit = (sample_rate as u64 * SILENCE_MS / 1_000) as usize;
    let window_samples = ((sample_rate as u64 * VAD_CHUNK_MS / 1_000) as usize).max(1);

    thread::spawn(move || {
        let mut watcher = SilenceWatcher::new(silence_limit);
        let mut processed = 0usize;

        loop {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
            if stop.load(Ordering::SeqCst) {
                return;
            }

            // Copia sólo lo nuevo y deja el buffer intacto para el WAV.
            let pending = {
                let guard = samples.lock().unwrap();
                if guard.len() <= processed {
                    continue;
                }
                let pending = guard[processed..].to_vec();
                processed = guard.len();
                pending
            };

            for piece in pending.chunks(window_samples) {
                watcher.feed(is_speech(piece), piece.len());
                if watcher.should_stop() {
                    // Un único `take` decide quién guarda: el botón o esto.
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
fn finish_session(session: Session, documents_dir: &Path, weak: &slint::Weak<AppWindow>) {
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

    // El motor real llega con la historia #4; hasta entonces se avisa.
    let model_path = documents_dir.join("modelo.onnx");
    let texto = match engine::transcribe(&model_path, &[]) {
        Ok(texto) => texto,
        Err(err) => {
            eprintln!("{err}");
            format!("[sin modelo: {err}]")
        }
    };

    append_note(documents_dir, &date, &time, &texto, &audio_rel)
}

/// Cambia el estado visible de la ventana desde cualquier hilo.
fn set_recording(weak: &slint::Weak<AppWindow>, recording: bool) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = weak.upgrade() {
            window.set_recording(recording);
        }
    });
}

slint::include_modules!();

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

        // 1,02 s de voz fingida (34 trozos de 30 ms) y 1,2 s de silencio a
        // 16 kHz: lo que queda después del corte también tiene que estar en
        // el WAV.
        let mut samples: Vec<i16> = (0..16_320)
            .map(|i| if i % 2 == 0 { 4_000 } else { -4_000 })
            .collect();
        samples.extend(vec![0i16; 19_200]);

        // Mismo criterio que el watcher: 480 muestras por trozo (30 ms).
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
