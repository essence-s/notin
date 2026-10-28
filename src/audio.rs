use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use thiserror::Error;

/// Mantiene viva la captura: mientras exista, el micrófono llena el
/// sumidero de muestras. Al soltarla (`drop`), cpal detiene su hilo.
pub struct Capture {
    // No se lee nunca directamente: vivir aquí es lo que mantiene la
    // captura abierta; al soltar `Capture` con drop, cpal cierra el hilo.
    #[allow(dead_code)]
    stream: cpal::Stream,
    sample_rate: u32,
}

impl Capture {
    /// El rate real del micrófono: será el de la cabecera del WAV.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("no hay micrófono de entrada disponible")]
    NoInputDevice,
    #[error("error de audio del sistema")]
    Audio(#[from] cpal::Error),
}

/// Arranca la captura del micro por defecto hacia `sink`.
/// cpal entrega f32 en [-1.0, 1.0]: se mezcla a mono y se convierte a i16.
/// La callback de cpal es 'static y corre en su propio hilo, por eso el
/// sumidero llega como `Arc<Mutex<...>>`.
pub fn start_capture(sink: Arc<Mutex<Vec<i16>>>) -> Result<Capture, CaptureError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(CaptureError::NoInputDevice)?;
    let config = device.default_input_config()?;
    let channels = usize::from(config.channels());
    let sample_rate = config.sample_rate();
    let stream = device.build_input_stream(
        cpal::StreamConfig {
            channels: config.channels(),
            sample_rate,
            buffer_size: cpal::BufferSize::Default,
        },
        move |data: &[f32], _info: &cpal::InputCallbackInfo| {
            // Mezcla por frames a mono y convierte a i16 en 16 bits.
            let mono: Vec<i16> = data
                .chunks(channels)
                .map(|frame| {
                    let sum: f32 = frame.iter().sum();
                    (sum / channels as f32 * i16::MAX as f32) as i16
                })
                .collect();
            sink.lock()
                .expect("el hilo de captura rompió el sumidero")
                .extend_from_slice(&mono);
        },
        |err| eprintln!("error de captura de audio: {err}"),
        None,
    )?;
    stream.play()?;
    Ok(Capture { stream, sample_rate })
}

/// Lee un WAV y devuelve las muestras como f32 en [-1.0, 1.0].
pub fn read_wav(path: &std::path::Path) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let mut reader = hound::WavReader::new(file)?;
    let mut samples = Vec::with_capacity(reader.len() as usize);
    for sample in reader.samples::<i16>() {
        let s = sample?;
        samples.push(s as f32 / i16::MAX as f32);
    }
    Ok(samples)
}
