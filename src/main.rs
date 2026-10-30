use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use slint::ComponentHandle;
#[cfg(target_os = "linux")]
use slint::Timer;

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
use ipc::{IpcCommand, IpcConnection};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let Some(documents_dir) = dirs::document_dir().map(|d| d.join("notin")) else {
        eprintln!("no encuentro la carpeta de Documentos de tu cuenta");
        return Err("no hay carpeta de Documentos".into());
    };

    match cli.cmd {
        Some(Cmd::Models) => cli::run_models(&documents_dir),
        Some(Cmd::Transcribe { archivo }) => cli::run_transcribe(&documents_dir, &archivo),
        None => run_app(documents_dir),
    }
}

/// Única función de arranque: declara ambas ventanas una vez, muestra la
/// flotante de dictado (`NotinWindow`) y oculta la de configuración tras un
/// warmup de 300 ms para Wayland. Si no hay otra instancia, actúa como
/// servidor IPC y ejecuta los comandos directamente dentro del event loop de
/// Slint (sin mpsc ni Timer de polling).
fn run_app(documents_dir: std::path::PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    match ipc::connect_or_serve(IpcCommand::OpenNotin)? {
        IpcConnection::Primary(server) => {
            let configuration_window = ConfigurationWindow::new()?;
            let notin_window = NotinWindow::new()?;

            let configuration_weak = configuration_window.as_weak();
            let notin_weak = notin_window.as_weak();

            // Visibilidad inicial: NotinWindow visible, ConfigurationWindow
            // visible también por un instante (warmup de Wayland) y luego
            // ocultada tras un temporizador de 300 ms. En Windows el warmup
            // no es necesario y solo causaría un parpadeo breve al arrancar.
            let _ = notin_window.show();

            #[cfg(target_os = "linux")]
            {
                // Wayland: show() de una ventana nunca mostrada es ignorado
                // hasta que el event loop procese un frame. Mostramos ambas
                // y ocultamos ConfigurationWindow tras 300 ms para darle
                // tiempo a crear su superficie XDG.
                let _ = configuration_window.show();
                let configuration_weak_warmup = configuration_weak.clone();
                let warmup_timer = Timer::default();
                warmup_timer.start(
                    slint::TimerMode::SingleShot,
                    Duration::from_millis(300),
                    move || {
                        if let Some(w) = configuration_weak_warmup.upgrade() {
                            let _ = w.hide();
                        }
                    },
                );
            }

            #[cfg(not(target_os = "linux"))]
            {
                // En Windows no hay surface de Wayland; dejar la ventana de
                // configuración oculta desde el inicio evita parpadeos.
                let _ = configuration_window.hide();
            }

            let dictation = Rc::new(Dictation::new(documents_dir.clone(), notin_weak.clone()));
            notin_window.on_toggle({
                let dictation = Rc::clone(&dictation);
                move || dictation.toggle()
            });

            let notin_visible = Arc::new(AtomicBool::new(true));
            let configuration_visible = Arc::new(AtomicBool::new(false));

            let notin_weak_ipc = notin_weak.clone();

            // Servidor IPC en hilo separado: acepta conexiones y despacha
            // cada comando al event loop de Slint mediante
            // invoke_from_event_loop, así show()/hide() siempre se ejecutan
            // sobre el hilo del event loop.
            std::thread::spawn(move || {
                server.run(Box::new(move |cmd| {
                    match cmd {
                        IpcCommand::OpenNotin => {
                            let notin_weak = notin_weak_ipc.clone();
                            let notin_visible = notin_visible.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(w) = notin_weak.upgrade() {
                                    let new_vis = !notin_visible.load(Ordering::SeqCst);
                                    if new_vis {
                                        let _ = w.show();
                                    } else {
                                        let _ = w.hide();
                                    }
                                    notin_visible.store(new_vis, Ordering::SeqCst);
                                }
                            });
                        }
                    }
                }));
            });

            // Rutas y estado inicial de la ventana de configuración.
            let model_dir = documents_dir.join("models");
            let model_path = model_dir.to_string_lossy().into_owned();
            configuration_window.set_model_path(model_path.into());
            configuration_window.set_models_installed(models::model_installed(&model_dir));

            // Abrir la ventana flotante desde la configuración.
            let notin_weak_cb = notin_weak.clone();
            configuration_window.on_open_float_window({
                move || {
                    let weak = notin_weak_cb.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            let _ = w.show();
                        }
                    });
                }
            });

            // Botón de tuerca: abre/oculta la configuración localmente (sin IPC).
            let configuration_weak_cb = configuration_weak.clone();
            let configuration_visible_cb = configuration_visible.clone();
            notin_window.on_open_configuration({
                move || {
                    let weak = configuration_weak_cb.clone();
                    let visible = configuration_visible_cb.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            let new_vis = !visible.load(Ordering::SeqCst);
                            if new_vis {
                                let _ = w.show();
                            } else {
                                let _ = w.hide();
                            }
                            visible.store(new_vis, Ordering::SeqCst);
                        }
                    });
                }
            });

            // Descarga de modelos.
            let configuration_weak_dl = configuration_weak.clone();
            let model_dir_dl = model_dir.clone();
            configuration_window.on_download_models({
                move || {
                    let weak = configuration_weak_dl.clone();
                    let model_dir = model_dir_dl.clone();
                    std::thread::spawn(move || {
                        match models::download_models(&model_dir) {
                            Ok(()) => {
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(w) = weak.upgrade() {
                                        w.set_models_installed(true);
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

            // No salir al cerrar/ocultar la última ventana (necesario en Wayland).
            let _ = i_slint_backend_selector::with_platform(|platform| {
                platform.set_event_loop_quit_on_last_window_closed(false);
                Ok(())
            });

            eprintln!("[app] iniciando event loop");
            slint::run_event_loop_until_quit()?;
            eprintln!("[app] event loop terminado");

            Ok(())
        }
        IpcConnection::Secondary => {
            // Segunda instancia: ya envió OpenNotin al conectar, solo salir.
            Ok(())
        }
    }
}

slint::include_modules!();
