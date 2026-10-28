use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

// Añade una línea `- HH:MM:SS — texto [audio](...)` al Markdown del día.
// Crea el fichero si no existe; si ya existe, añade al final.
pub fn append_note(
    dir: &Path,
    date: &str,
    time: &str,
    text: &str,
    audio_rel: &str,
) -> io::Result<PathBuf> {
    let path = dir.join(format!("{date}.md"));
    let line = format!("- {time} — {text} [audio]({audio_rel})\n");

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(line.as_bytes())?;

    Ok(path)
}

/// Mueve el WAV grabado a la subcarpeta `audio/` de la carpeta de salida y
/// devuelve la ruta relativa para el enlace del markdown.
pub fn move_wav_to_audio_folder(dir: &Path, wav_path: &Path) -> io::Result<String> {
    let audio_dir = dir.join("audio");
    fs::create_dir_all(&audio_dir)?;
    let file_name = wav_path.file_name().expect("el fichero wav tiene nombre");
    let dest = audio_dir.join(file_name);
    fs::rename(wav_path, &dest)?;
    Ok(format!("audio/{}", file_name.to_string_lossy()))
}

// Prueba 1 del par: `appends_transcribed_line_with_audio_link_to_daily_md`.
// Dado una fecha, hora, texto y ruta relativa del audio, cuando se añade la
// nota, entonces aparece la línea `- HH:MM:SS — texto [audio](...)` en el
// Markdown del día.
#[cfg(test)]
mod store_tests {
    use std::fs;

    #[test]
    fn appends_transcribed_line_with_audio_link_to_daily_md() {
        let dir = std::env::temp_dir().join("notin_test_store_append_note");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crea carpeta temporal");

        let path = crate::store::append_note(
            &dir,
            "2026-10-28",
            "15:04:22",
            "esta es una prueba",
            "audio/locucion-2026-10-28-150422.wav",
        )
        .expect("escribe la nota");

        assert_eq!(path, dir.join("2026-10-28.md"));

        let content = fs::read_to_string(&path).expect("lee el md");
        assert_eq!(
            content,
            "- 15:04:22 — esta es una prueba [audio](audio/locucion-2026-10-28-150422.wav)\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    // Prueba 2 del par: `places_wav_in_audio_subfolder_and_returns_relative_path`.
    // Dado un WAV y la carpeta destino, entonces lo deja en audio/ y devuelve
    // la ruta relativa para el enlace del markdown.
    #[test]
    fn places_wav_in_audio_subfolder_and_returns_relative_path() {
        use std::fs;

        let dir = std::env::temp_dir().join("notin_test_store_audio_subfolder");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crea carpeta temporal");

        let wav_path = dir.join("locucion-temporal.wav");
        fs::write(&wav_path, b"RIFF....WAVE....").expect("escribe wav temporal");

        let rel = crate::store::move_wav_to_audio_folder(&dir, &wav_path)
            .expect("mueve el wav a audio/");

        assert_eq!(rel, "audio/locucion-temporal.wav");
        assert!(
            dir.join("audio/locucion-temporal.wav").exists(),
            "el wav debe estar en audio/"
        );
        assert!(
            !wav_path.exists(),
            "el wav original ya no debe estar en la raíz"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
