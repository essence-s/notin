use std::rc::Rc;

use slint::ComponentHandle;

mod audio;
mod dictation;
mod engine;
mod recorder;
mod store;
mod vad;

use dictation::Dictation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = AppWindow::new()?;

    // Carpeta por defecto decidida: ~/Documentos/notin (en Windows, la de
    // Documentos del perfil). Sin config todavía: eso es la historia #6.
    let Some(documents_dir) = dirs::document_dir().map(|d| d.join("notin")) else {
        eprintln!("no encuentro la carpeta de Documentos de tu cuenta");
        return Err("no hay carpeta de Documentos".into());
    };

    let dictation = Rc::new(Dictation::new(documents_dir, window.as_weak()));

    window.on_toggle({
        let dictation = Rc::clone(&dictation);
        move || dictation.toggle()
    });

    Ok(window.run()?)
}

slint::include_modules!();
