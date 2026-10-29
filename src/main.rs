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
        Some(Cmd::Dictado) => run_dictado(documents_dir),
        None => run_gui(documents_dir),
    }
}

/// Lanza la ventana flotante secundaria (por CLI o desde la configuración).
fn run_dictado(documents_dir: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let window = DictadoWindow::new()?;
    let dictation = Rc::new(Dictation::new(documents_dir, window.as_weak()));

    window.on_toggle({
        let dictation = Rc::clone(&dictation);
        move || dictation.toggle()
    });

    // Sonido y ventana: un hilo para cada cosa.
    // Toda locución nace y muere en el botón.
    // y al pararlo, lo hablado 
    // va a un WAV.
    Ok(window.run()?)
}

/// Lanza la ventana principal de configuración.
fn run_gui(documents_dir: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let window = NotinWindow::new()?;

    let model_dir = documents_dir.join("models");
    let model_path = model_dir.to_string_lossy().into_owned();

    window.set_model_path(model_path.into());
    window.set_models_installed(models::model_installed(&model_dir));

    let window_weak = window.as_weak();

    window.on_download_models({
        let model_dir = model_dir.clone();
        let window_weak = window_weak.clone();
        move || {
            let window_weak = window_weak.clone();
            let model_dir = model_dir.clone();
            std::thread::spawn(move || {
                match models::download_models(&model_dir) {
                    Ok(()) => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(window) = window_weak.upgrade() {
                                window.set_models_installed(true);
                            }
                        });
                    }
                    Err(err) => {
                        eprintln!("error al descargar modelos: {err}");
                    }
                }
            });
        }
    });

    window.on_open_float_window({
        let documents_dir = documents_dir.clone();
        move || {
            let dictado_window = DictadoWindow::new().expect("crea ventana flotante");
            let dictation = Rc::new(Dictation::new(documents_dir.clone(), dictado_window.as_weak()));
            dictado_window.on_toggle({
                let dictation = Rc::clone(&dictation);
                move || dictation.toggle()
            });
            dictado_window.show().expect("muestra ventana flotante");
        }
    });

    Ok(window.run()?)
}

slint::include_modules!();
