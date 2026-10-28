use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

mod audio;
mod recorder;

mod store;

mod engine;

use chrono::Local;
use recorder::Recorder;
use store::{append_note, move_wav_to_audio_folder};

/// Una sesión de grabación en vivo: mientras existe, el micro está
/// alimentando el sumidero. Se suelta para parar y guardar.
struct Session {
    capture: audio::Capture,
    samples: Arc<Mutex<Vec<i16>>>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = AppWindow::new()?;
    let weak = window.as_weak();

    // Carpeta por defecto decidida: ~/Documentos/notin (en Windows,
    // la de Documentos del perfil). Sin config todavía: eso es la historia #6.
    let Some(documents_dir) = dirs::document_dir().map(|d| d.join("notin")) else {
        eprintln!("no encuentro la carpeta de Documentos de tu cuenta");
        return Err("no hay carpeta de Documentos".into());
    };

    // Session está sólo en el hilo de UI (los callbacks lo son), así que
    // Rc<RefCell> basta.
    let session: Rc<RefCell<Option<Session>>> = Rc::new(RefCell::new(None));

    window.on_toggle({
        let session = Rc::clone(&session);
        move || {
            // El `RefMut` debe morir aquí: si se borrowa dentro del match,
            // la rama de "empezar" choca con él ("RefCell already borrowed").
            // Con el `let`, el temporal cae en el `;` y el `take()` deja
            // libre el RefCell para lo que venga.
            let ongoing = session.borrow_mut().take();
            match ongoing {
                // Hay grabación en marcha: parar (soltar la captura) y guardar.
                Some(Session { capture, samples }) => {
                    let sample_rate = capture.sample_rate();
                    drop(capture);

                    let recorded = samples.lock().expect("el hilo de captura rompió el sumidero");
                    let mut locution = Recorder::new(documents_dir.clone(), sample_rate);
                    locution.record(&recorded);
                    let wav_path = match locution.stop() {
                        Ok(path) => path,
                        Err(err) => {
                            eprintln!("error al guardar el wav: {err}");
                            return;
                        }
                    };

                    // Fecha y hora para el nombre del día y la línea del markdown.
                    let now = Local::now();
                    let date = now.format("%Y-%m-%d").to_string();
                    let time = now.format("%H:%M:%S").to_string();

                    // Mover el wav a audio/ y obtener la ruta relativa para el enlace.
                    let audio_rel = match move_wav_to_audio_folder(&documents_dir, &wav_path) {
                        Ok(rel) => rel,
                        Err(err) => {
                            eprintln!("error al mover el wav a audio/: {err}");
                            return;
                        }
                    };

                    // Intentar transcribir; si falta el modelo, se ve un mensaje
                    // claro y no crashea (historia #2, criterio 3).
                    let model_path = documents_dir.join("modelo.onnx"); // provisional hasta #4
                    let texto = match engine::transcribe(&model_path, &[]) {
                        Ok(t) => t,
                        Err(err) => {
                            eprintln!("{err}");
                            format!("[sin modelo: {err}]")
                        }
                    };

                    if let Err(err) = append_note(&documents_dir, &date, &time, &texto, &audio_rel) {
                        eprintln!("error al escribir la nota: {err}");
                    }
                }
                // En reposo: empezar a grabar.
                None => {
                    let samples = Arc::new(Mutex::new(Vec::new()));
                    match audio::start_capture(Arc::clone(&samples)) {
                        Ok(capture) => {
                            *session.borrow_mut() = Some(Session { capture, samples });
                        }
                        Err(err) => eprintln!("no se pudo grabar: {err}"),
                    }
                }
            }
            let recording = session.borrow().is_some();
            if let Some(window) = weak.upgrade() {
                window.set_recording(recording);
            }
        }
    });

    // Sonido y ventana: un hilo para cada cosa.
    // Toda locución nace y muere en el botón.
    // Y al pararlo, lo hablado va a un WAV, luego a audio/ y al Markdown del día.
    Ok(window.run()?)
}

slint::include_modules!();
