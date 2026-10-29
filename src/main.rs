use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use slint::ComponentHandle;

mod audio;
mod cli;
mod dictation;
mod engine;
mod ipc;
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
        Some(Cmd::Dictado { .. }) => run_dictado(documents_dir),
        None => run_gui(documents_dir),
    }
}

/// Lanza la ventana flotante secundaria (por CLI o desde la configuración).
fn run_dictado(documents_dir: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ipc::acquire_lock(&documents_dir)?;

    let window = DictadoWindow::new()?;
    let dictation = Rc::new(Dictation::new(documents_dir.clone(), window.as_weak()));
    window.on_toggle({
        let dictation = Rc::clone(&dictation);
        move || dictation.toggle()
    });

    let visible = Arc::new(AtomicBool::new(true));
    let visible_clone = visible.clone();
    let window_weak = window.as_weak();
    let signal_dir = documents_dir.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(200));
            if window_weak.upgrade().is_none() {
                break;
            }
            if ipc::consume_toggle_signal(&signal_dir) {
                let visible = visible_clone.clone();
                let window_weak = window_weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = window_weak.upgrade() {
                        if visible.load(Ordering::SeqCst) {
                            let _ = window.hide();
                            visible.store(false, Ordering::SeqCst);
                        } else {
                            let _ = window.show();
                            visible.store(true, Ordering::SeqCst);
                        }
                    }
                });
            }
        }
    });

    Ok(window.run()?)
}

/// Lanza la ventana principal de configuración.
fn run_gui(documents_dir: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let _lock = ipc::acquire_lock(&documents_dir)?;

    let window = NotinWindow::new()?;

    let model_dir = documents_dir.join("models");
    let model_path = model_dir.to_string_lossy().into_owned();

    window.set_model_path(model_path.into());
    window.set_models_installed(models::model_installed(&model_dir));

    let window_weak = window.as_weak();

    // Crear DictadoWindow oculto inicialmente para el monitoreo de señal
    let dictado_window = DictadoWindow::new()?;
    let dictado_weak = dictado_window.as_weak();
    let dictation = Rc::new(Dictation::new(documents_dir.clone(), dictado_weak.clone()));
    dictado_window.on_toggle({
        let dictation = Rc::clone(&dictation);
        move || dictation.toggle()
    });

    let visible = Arc::new(AtomicBool::new(false));
    let visible_clone = visible.clone();
    let signal_dir = documents_dir.clone();
    let dictado_weak_for_monitor = dictado_weak.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(200));
            if dictado_weak_for_monitor.upgrade().is_none() {
                break;
            }
            if ipc::consume_toggle_signal(&signal_dir) {
                let visible = visible_clone.clone();
                let dictado_weak = dictado_weak_for_monitor.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(window) = dictado_weak.upgrade() {
                        if visible.load(Ordering::SeqCst) {
                            let _ = window.hide();
                            visible.store(false, Ordering::SeqCst);
                        } else {
                            let _ = window.show();
                            visible.store(true, Ordering::SeqCst);
                        }
                    }
                });
            }
        }
    });

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
        let dictado_weak = dictado_weak.clone();
        let visible = visible.clone();
        move || {
            if let Some(window) = dictado_weak.upgrade() {
                        if visible.load(Ordering::SeqCst) {
                            let _ = window.hide();
                            visible.store(false, Ordering::SeqCst);
                        } else {
                            let _ = window.show();
                            visible.store(true, Ordering::SeqCst);
                        }
            }
        }
    });

    Ok(window.run()?)
}

slint::include_modules!();
