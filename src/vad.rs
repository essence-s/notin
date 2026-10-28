// Dado un trozo de audio con voz y otro con silencio, devuelve la
// clasificación correcta.
pub fn is_speech(samples: &[i16]) -> bool {
    // Umbral de energía: si alguna muestra supera el umbral, hay voz.
    samples.iter().any(|&s| s.abs() > 1000)
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
