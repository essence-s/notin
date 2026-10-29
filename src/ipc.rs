//! Señal de toggle entre procesos para `notin dictado toggle`.
//! Cuando el usuario ejecuta `notin dictado toggle` y ya hay una instancia
//! corriendo, se escribe un archivo de señal que la instancia principal
//! detecta para alternar la visibilidad de la ventana flotante.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// Escribe un archivo de señal para pedir a la instancia corriente que alterne
/// la ventana flotante de dictado.
pub fn toggle_signal(documents_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let signal_path = documents_dir.join(".toggle_dictado");
    fs::write(&signal_path, "toggle")?;
    Ok(())
}

/// Bloqueo de instancia única: mantiene `.notin.lock` mientras vive el guardia.
pub struct LockGuard {
    path: PathBuf,
}

/// Intenta adquirir el bloqueo de instancia única en `documents_dir`.
/// Devuelve error si ya hay otra instancia con el bloqueo.
pub fn acquire_lock(documents_dir: &Path) -> Result<LockGuard, Box<dyn std::error::Error>> {
    let lock_path = documents_dir.join(".notin.lock");
    let _file = fs::File::create_new(&lock_path)?;
    Ok(LockGuard { path: lock_path })
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod ipc_tests {
    use super::*;
    use std::fs;

    #[test]
    fn toggle_signal_creates_signal_file() {
        let dir = std::env::temp_dir().join("notin_test_toggle_signal");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crea carpeta temporal");

        toggle_signal(&dir).expect("crea señal de toggle");

        assert!(
            dir.join(".toggle_dictado").exists(),
            "archivo de señal debe existir"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn acquire_lock_returns_lock_when_no_instance() {
        let dir = std::env::temp_dir().join("notin_test_lock_no_instance");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crea carpeta temporal");

        let lock = acquire_lock(&dir).expect("adquiere lock");
        assert!(dir.join(".notin.lock").exists());

        drop(lock);
        assert!(!dir.join(".notin.lock").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn acquire_lock_returns_err_when_instance_exists() {
        let dir = std::env::temp_dir().join("notin_test_lock_instance");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crea carpeta temporal");

        let _lock1 = acquire_lock(&dir).expect("primer lock");
        let result = acquire_lock(&dir);
        assert!(result.is_err(), "segundo lock debe fallar");

        let _ = fs::remove_dir_all(&dir);
    }
}
