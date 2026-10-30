use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::audio;
use crate::engine;
use crate::models;

/// CLI de notin: descarga modelos, transcribe un WAV o lanza la GUI.
#[derive(Parser, Debug)]
#[command(name = "notin", version, about = "Dictado offline: hablas y queda en texto en un Markdown")]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug, Clone, PartialEq)]
pub enum Cmd {
    /// Descarga los modelos offline (Nemo CTC + VAD Silero).
    Models,
    /// Transcribe un WAV. Sirve para probar el motor sin micrófono.
    Transcribe {
        /// Ruta del fichero WAV.
        archivo: PathBuf,
    },
}

/// Ejecuta `notin models`: descarga Nemo CTC + Silero VAD al directorio de modelos.
pub fn run_models(documents_dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let model_dir = documents_dir.join("models");
    models::download_models(&model_dir)?;
    Ok(())
}

/// Ejecuta `notin transcribe <archivo>`: transcribe un WAV y imprime el texto.
pub fn run_transcribe(
    documents_dir: &std::path::Path,
    archivo: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let model_dir = documents_dir.join("models");
    let model_path = model_dir.join("nemo-ctc-es").join("model.int8.onnx");
    let samples = audio::read_wav(archivo)?;
    let texto = engine::transcribe(&model_path, &samples)?;
    println!("{}", texto.trim());
    Ok(())
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    #[test]
    fn cli_parse_models() {
        let cli = Cli::try_parse_from(["notin", "models"]).expect("parses models");
        assert!(matches!(cli.cmd, Some(Cmd::Models)));
    }

    #[test]
    fn cli_parse_transcribe() {
        let cli = Cli::try_parse_from(["notin", "transcribe", "audio.wav"])
            .expect("parses transcribe");
        assert!(matches!(
            cli.cmd,
            Some(Cmd::Transcribe { archivo })
                if archivo == PathBuf::from("audio.wav")
        ));
    }

    #[test]
    fn cli_parse_no_args_defaults_to_none() {
        let cli = Cli::try_parse_from(["notin"]).expect("parses no args");
        assert_eq!(cli.cmd, None);
    }
}
