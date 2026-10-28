use std::rc::Rc;

use clap::Parser;
use slint::ComponentHandle;

mod audio;
mod cli;
mod dictation;
mod engine;
mod models;
mod recorder;
mod resample;
mod store;
mod vad;

use cli::{Cli, Cmd};
use dictation::Dictation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let Some(documents_dir) = dirs::document_dir().map(|d| d.join("notin")) else {
        eprintln!("no encuentro la carpeta de Documentos de tu cuenta");
        return Err("no hay carpeta de Documentos".into());
    };

    match cli.cmd {
        Some(Cmd::Models) => cli::run_models(&documents_dir),
        Some(Cmd::Transcribe { archivo }) => cli::run_transcribe(&documents_dir, &archivo),
        None => run_gui(documents_dir),
    }
}

/// Lanza la GUI (ventana flotante con botón).
fn run_gui(documents_dir: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let window = AppWindow::new()?;
    let dictation = Rc::new(Dictation::new(documents_dir, window.as_weak()));

    window.on_toggle({
        let dictation = Rc::clone(&dictation);
        move || dictation.toggle()
    });

    Ok(window.run()?)
}

slint::include_modules!();
