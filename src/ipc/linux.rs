//! IPC en Linux mediante Unix domain socket.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use super::IpcCommand;
use super::IpcServer;

const SOCKET_NAME: &str = "notin-ipc.sock";

/// Ruta del socket IPC.
fn socket_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .map(|d| std::path::PathBuf::from(d))
        .or_else(|| dirs::cache_dir().map(|d| d.join("notin")));

    let dir = match runtime_dir {
        Some(d) => {
            fs::create_dir_all(&d)?;
            d
        }
        None => {
            let d = std::env::temp_dir().join("notin");
            fs::create_dir_all(&d)?;
            d
        }
    };

    Ok(dir.join(SOCKET_NAME))
}

/// Servidor de IPC en Linux.
pub struct LinuxIpcServer {
    listener: UnixListener,
}

impl LinuxIpcServer {
    pub fn new(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if path.exists() {
            fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        Ok(Self { listener })
    }
}

impl IpcServer for LinuxIpcServer {
    fn run(&self, on_command: Box<dyn Fn(IpcCommand) + Send + Sync + 'static>) {
        let on_command = Arc::new(on_command);
        for stream in self.listener.incoming().flatten() {
            let on_command = on_command.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                if reader.read_line(&mut line).is_ok() {
                    let cmd = match line.trim() {
                        "open-notin" => IpcCommand::OpenNotin,
                        _ => return,
                    };
                    on_command(cmd);
                }
            });
        }
    }
}

/// Intenta conectar al servidor existente enviando `cmd`.
/// Si conecta, devuelve Secondary (quien llama debe salir).
/// Si no hay servidor, crea uno y devuelve Primary (quien llama lo ejecuta).
pub fn connect_or_serve(cmd: super::IpcCommand) -> Result<super::IpcConnection, Box<dyn std::error::Error>> {
    let path = socket_path()?;
    let msg = format!("{}\n", cmd.as_str());

    // Intentar conectar como cliente a un servidor existente.
    // Reintenta hasta 10 veces con 50 ms de espera para manejar el caso
    // donde el servidor aún está iniciando (evita race condition).
    for _ in 0..10 {
        if let Ok(mut stream) = UnixStream::connect(&path) {
            stream.write_all(msg.as_bytes())?;
            return Ok(super::IpcConnection::Secondary);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // No hay servidor: esta es la primera instancia
    let server = LinuxIpcServer::new(&path)?;
    Ok(super::IpcConnection::Primary(Box::new(server)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_returns_a_path() {
        let path = socket_path().expect("obtiene ruta del socket");
        assert_eq!(path.file_name().unwrap(), SOCKET_NAME);
    }
}
