//! Resamplea audio de cualquier rate a 16 kHz para el motor y el VAD.

use rubato::audioadapter_buffers::direct::SequentialSlice;
use rubato::{Fft, FixedSync, Resampler as _};

/// Reajusta el audio a la frecuencia que espera el modelo (16 kHz).
pub struct Resampler {
    inner: Option<Fft<f32>>,
    pending: Vec<f32>,
}

impl Resampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Result<Self, String> {
        if from_rate == to_rate {
            return Ok(Self { inner: None, pending: Vec::new() });
        }
        let chunk = ((from_rate as usize) / 50).max(64);
        let inner = Fft::<f32>::new(from_rate as usize, to_rate as usize, chunk, 1, FixedSync::Input)
            .map_err(|e| format!("no se pudo crear el resampleador: {e}"))?;
        Ok(Self { inner: Some(inner), pending: Vec::new() })
    }

    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>, String> {
        let Some(inner) = self.inner.as_mut() else {
            return Ok(input.to_vec());
        };
        self.pending.extend_from_slice(input);
        let need = inner.input_frames_next();
        let mut out = Vec::with_capacity(self.pending.len() / 3 + 16);
        while self.pending.len() >= need {
            let chunk: Vec<f32> = self.pending.drain(..need).collect();
            let adapter = SequentialSlice::new(&chunk, 1, need)
                .map_err(|e| format!("no se pudo preparar el audio: {e}"))?;
            let res = inner.process(&adapter, None).map_err(|e| format!("falló el resampleo: {e}"))?;
            out.extend_from_slice(&res.take_data());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_converts_48k_to_16k() {
        let mut resampler = Resampler::new(48_000, 16_000).expect("crea resampleador");
        let input: Vec<f32> = vec![0.5; 4800];
        let output = resampler.process(&input).expect("resamplea");
        assert!(
            output.len() >= 1400 && output.len() <= 1650,
            "100 ms a 48 kHz ≈ 100 ms a 16 kHz (≈1600 muestras), obtuvo {}",
            output.len()
        );
    }

    #[test]
    fn resample_pasa_silencio_a_silencio() {
        let mut resampler = Resampler::new(48_000, 16_000).expect("crea resampleador");
        let input: Vec<f32> = vec![0.0; 4800];
        let output = resampler.process(&input).expect("resamplea");
        assert!(
            output.iter().all(|&s| s == 0.0),
            "silencio sigue siendo silencio"
        );
    }
}
