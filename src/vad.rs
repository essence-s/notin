// Dado un trozo de audio con voz y otro con silencio, devuelve la
// clasificación correcta.
#[allow(dead_code)]
pub fn is_speech(samples: &[i16]) -> bool {
    // Umbral de energía: si alguna muestra supera el umbral, hay voz.
    samples.iter().any(|&s| s.abs() > 1000)
}

use std::path::Path;

use sherpa_rs::silero_vad::{SileroVad, SileroVadConfig};

/// Wrapper del VAD Silero que verifica la existencia del modelo antes de cargar.
/// El audio debe ir ya a 16 kHz mono (resampleado).
#[derive(Debug)]
pub struct VadDetector {
    inner: Option<SileroVad>,
}

impl VadDetector {
    pub fn new(model_path: &Path) -> Result<Self, String> {
        if !model_path.exists() {
            return Err(format!("no se encontró el modelo de VAD en {:?}", model_path));
        }
        let config = SileroVadConfig {
            model: model_path.to_string_lossy().into_owned(),
            min_silence_duration: 0.5,
            min_speech_duration: 0.1,
            max_speech_duration: 5.0,
            threshold: 0.5,
            sample_rate: 16_000,
            window_size: 512,
            provider: None,
            num_threads: Some(1),
            debug: false,
        };
        let inner = SileroVad::new(config, 1.0)
            .map_err(|e| format!("no se pudo crear el VAD Silero: {e}"))?;
        Ok(Self { inner: Some(inner) })
    }

    /// Alimenta un trozo de audio (f32, 16 kHz mono) y devuelve si hay voz.
    pub fn is_speech(&mut self, samples: &[f32]) -> bool {
        let Some(inner) = self.inner.as_mut() else {
            return false;
        };
        inner.accept_waveform(samples.to_vec());
        inner.is_speech()
    }
}

/// Vigila el silencio en muestras y avisa cuándo hay suficiente silencio
/// seguido como para parar. Antes de la primera voz no cuenta: así no corta
/// nada más empezar a grabar.
pub struct SilenceWatcher {
    silence_limit: usize,
    silent_samples: usize,
    heard_speech: bool,
}

impl SilenceWatcher {
    /// Crea el vigilante: se para al acumular `silence_limit` muestras
    /// silenciosas seguidas después de haber oído voz.
    pub fn new(silence_limit: usize) -> Self {
        SilenceWatcher {
            silence_limit,
            silent_samples: 0,
            heard_speech: false,
        }
    }

    /// Alimenta la clasificación de un trozo y cuántas muestras trae.
    pub fn feed(&mut self, has_speech: bool, samples: usize) {
        if has_speech {
            self.heard_speech = true;
            self.silent_samples = 0;
        } else if self.heard_speech {
            self.silent_samples += samples;
        }
    }

    /// Devuelve true si se alcanzó el umbral de silencio seguido.
    pub fn should_stop(&self) -> bool {
        self.heard_speech && self.silent_samples >= self.silence_limit
    }
}

// Prueba del par: `silence_watcher_triggers_stop_after_threshold`.
// Dada una secuencia de trozos clasificados, cuando se supera el umbral de
// silencio seguido, entonces devuelve should_stop = true; si no, false.
#[cfg(test)]
mod watcher_tests {
    use super::*;

    #[test]
    fn silence_watcher_triggers_stop_after_threshold() {
        let mut watcher = SilenceWatcher::new(1_000);

        // Voz seguida de silencio corto: sin parar.
        watcher.feed(true, 500);
        watcher.feed(false, 600);
        assert!(!watcher.should_stop(), "600 de 1000 no llegan");

        // Se completa el umbral: se activa el paro automático.
        watcher.feed(false, 400);
        assert!(watcher.should_stop(), "1000 muestras de silencio paran");
    }

    #[test]
    fn silence_watcher_does_not_stop_before_threshold() {
        let mut watcher = SilenceWatcher::new(1_000);
        watcher.feed(true, 500);
        watcher.feed(false, 999);
        assert!(!watcher.should_stop(), "999 de 1000 no paran");
    }

    #[test]
    fn silence_before_speech_never_stops() {
        let mut watcher = SilenceWatcher::new(1_000);
        watcher.feed(false, 10_000);
        assert!(
            !watcher.should_stop(),
            "sin voz no hay corte automático: se puede tardar en empezar"
        );
    }

    #[test]
    fn voice_resets_silence() {
        let mut watcher = SilenceWatcher::new(1_000);
        watcher.feed(true, 100);
        watcher.feed(false, 900);
        watcher.feed(true, 100);
        watcher.feed(false, 900);
        assert!(!watcher.should_stop(), "la voz reinicia el contador");
    }
}

#[cfg(test)]
mod vad_tests {
    use super::*;

    #[test]
    fn vad_detects_voice_and_silence() {
        // Silencio puro: no es voz.
        let silence = &[0i16; 160];
        assert!(!is_speech(silence), "el silencio no es voz");

        // Señal fuerte: es voz.
        let voice = &[0i16, 0, 4000, -4000, 0, 0];
        assert!(is_speech(voice), "una señal fuerte es voz");
    }
}

#[cfg(test)]
mod silero_vad_tests {
    use super::*;

    #[test]
    fn vad_silero_new_returns_error_when_model_missing() {
        let model_path = std::env::temp_dir().join("no_existe_silero_vad.onnx");
        let _ = std::fs::remove_file(&model_path);
        let result = VadDetector::new(&model_path);
        assert!(result.is_err(), "modelo Silero faltante debe devolver error");
        let err = result.unwrap_err();
        assert!(!err.is_empty(), "el mensaje de error no debe estar vacío");
    }

    #[test]
    #[ignore] // requiere silero_vad.onnx descargado (red / `notin models`)
    fn vad_silero_classifies_voice_and_silence() {
        let model_path = dirs::document_dir()
            .unwrap_or_default()
            .join("notin/models/silero_vad.onnx");
        let mut vad = VadDetector::new(&model_path).expect("carga silero_vad");

        let silence: Vec<f32> = vec![0.0; 1600];
        assert!(!vad.is_speech(&silence), "silencio no es voz");

        let voice: Vec<f32> = (0..1600).map(|i| (i as f32 / 1600.0) * 0.5).collect();
        assert!(vad.is_speech(&voice), "señal de voz es voz");
    }
}
