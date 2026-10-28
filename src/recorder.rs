use std::fs;
use std::io;
use std::path::PathBuf;

/// Una locución: acumula las muestras capturadas por el micrófono y
/// al parar las guarda como un WAV en la carpeta de salida.
///
/// La carpeta y el sample rate se inyectan para poder probar (es el rate
/// real del micro, decidido en el paso 1 del wiring). Después de `stop()`
/// la locución queda vacía y lista para otra: el botón se volverá a pulsar.
pub struct Recorder {
    output_dir: PathBuf,
    sample_rate: u32,
    samples: Vec<i16>,
}

impl Recorder {
    /// Crea la locución vacía; todavía no graba nada.
    pub fn new(output_dir: PathBuf, sample_rate: u32) -> Self {
        Recorder {
            output_dir,
            sample_rate,
            samples: Vec::new(),
        }
    }

    /// Añade las muestras que manda la captura de audio (en trozos).
    pub fn record(&mut self, samples: &[i16]) {
        self.samples.extend_from_slice(samples);
    }

    /// Para la locución y guarda el WAV. Devuelve la ruta del fichero.
    ///
    /// El nombre lleva fecha-hora local; si ya existiera (dos locuciones
    /// en el mismo segundo), añade un sufijo `-2`, `-3`... en vez de
    /// sobreescribir a ciegas.
    pub fn stop(&mut self) -> io::Result<PathBuf> {
        fs::create_dir_all(&self.output_dir)?;

        let base = chrono::Local::now().format("locucion-%Y-%m-%d-%H%M%S").to_string();
        let mut path = self.output_dir.join(format!("{base}.wav"));
        let mut duplicate = 1;
        while path.exists() {
            duplicate += 1;
            path = self.output_dir.join(format!("{base}-{duplicate}.wav"));
        }

        let spec = hound::WavSpec {
            channels: 1, // mezcla a mono: el wiring de captura ya lo garantiza
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer =
            hound::WavWriter::create(&path, spec).map_err(|err| io::Error::other(err.to_string()))?;
        for sample in &self.samples {
            writer
                .write_sample(*sample)
                .map_err(|err| io::Error::other(err.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|err| io::Error::other(err.to_string()))?;

        self.samples.clear();
        Ok(path)
    }
}

// Prueba 1 del par: `saves_wav_on_stop`.
// Dado que grabo muestras, cuando paro, entonces aparece un WAV válido
// en la carpeta de salida.
#[cfg(test)]
mod recorder_tests {
    /// Rate de 16 kHz para las pruebas, exactamente como el fijo anterior.
    const TEST_RATE: u32 = 16_000;

    #[test]
    fn saves_wav_on_stop() {
        use std::fs;

        // Carpeta temporal propia de la prueba (multiplataforma, se borra
        // al final). La ruta por defecto ~/Documentos/notin se cablea en
        // otro ciclo y se queda para la verificación manual.
        let dir = std::env::temp_dir().join("notin_test_saves_wav_on_stop");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crea carpeta temporal");

        let mut recorder = super::Recorder::new(dir.clone(), TEST_RATE);
        recorder.record(&[0; 1600]); // un segundo de silencio fingido

        let path = recorder.stop().expect("graba el WAV al parar");

        assert!(
            path.starts_with(&dir),
            "el WAV debe quedar en la carpeta dada: {}",
            path.display()
        );
        assert!(
            path.extension().is_some_and(|ext| ext == "wav"),
            "debe tener extensión .wav: {}",
            path.display()
        );
        // Se valida con hound: si la cabecera RIFF estuviera mal, fallará aquí.
        let reader = hound::WavReader::open(&path).expect("abre el WAV");
        assert_eq!(reader.spec().sample_format, hound::SampleFormat::Int);

        let _ = fs::remove_dir_all(&dir);
    }

    // Prueba 2 del par: `saved_wav_contains_recorded_samples`.
    // Dado que grabo muestras en trozos (como manda la captura de audio),
    // cuando paro, entonces el WAV contiene exactamente lo grabado,
    // muestra a muestra.
    #[test]
    fn saved_wav_contains_recorded_samples() {
        use std::fs;

        let dir = std::env::temp_dir().join("notin_test_saved_wav_contains_samples");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crea carpeta temporal");

        // Patrón reconocible y conocido de antemano: el valor esperado no
        // se calcula con la lógica del código, viene de esta lista fija.
        let samples: Vec<i16> = (0..1600)
            .map(|i| ((i % 1000) * 13 - 300) as i16)
            .collect();

        let mut recorder = super::Recorder::new(dir.clone(), TEST_RATE);
        recorder.record(&samples[..800]); // simula dos trozos del micrófono
        recorder.record(&samples[800..]);

        let path = recorder.stop().expect("graba el WAV al parar");

        let mut reader = hound::WavReader::open(&path).expect("abre el WAV");
        let recorded: Vec<i16> = reader
            .samples()
            .map(|sample| sample.expect("lee muestra"))
            .collect();

        assert_eq!(
            recorded.len(),
            samples.len(),
            "el WAV debe tener todas las muestras grabadas"
        );
        assert_eq!(recorded, samples, "muestra a muestra debe ser idéntico");

        let _ = fs::remove_dir_all(&dir);
    }
}
