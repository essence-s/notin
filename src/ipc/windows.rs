//! IPC en Windows mediante named pipe.

use std::sync::Arc;
use std::thread;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    ConnectNamedPipe, CreateFileW, CreateNamedPipeW, DisconnectNamedPipe, ReadFile, WriteFile,
    FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE, PIPE_WAIT,
};

use super::IpcCommand;
use super::IpcServer;

const PIPE_NAME: &str = r"\\.\pipe\notin-ipc";

/// Servidor de IPC en Windows.
pub struct WindowsIpcServer;

impl IpcServer for WindowsIpcServer {
    fn run(&self, on_command: Box<dyn Fn(IpcCommand) + Send + Sync + 'static>) {
        let on_command = Arc::new(on_command);

        loop {
            unsafe {
                let pipe = CreateNamedPipeW(
                    to_wide(PIPE_NAME).as_ptr(),
                    PIPE_ACCESS_DUPLEX,
                    PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                    1, // max instances
                    4096, // out buffer size
                    4096, // in buffer size
                    0, // default timeout
                    std::ptr::null_mut(), // security attributes
                );

                if pipe == INVALID_HANDLE_VALUE {
                    // Si no puede crear el pipe, esperar y reintentar
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }

                let connected = ConnectNamedPipe(pipe, std::ptr::null_mut()) != 0;

                if connected {
                    let on_command = on_command.clone();
                    thread::spawn(move || {
                        let mut buffer = [0u8; 4096];
                        let mut bytes_read: u32 = 0;

                        if ReadFile(
                            pipe,
                            buffer.as_mut_ptr() as *mut _,
                            buffer.len() as u32,
                            &mut bytes_read,
                            std::ptr::null_mut(),
                        ) != 0
                        {
                            let line = String::from_utf8_lossy(&buffer[..bytes_read as usize]);
                            let cmd = match line.trim() {
                                "open-notin" => IpcCommand::OpenNotin,
                                _ => return,
                            };
                            on_command(cmd);
                        }

                        DisconnectNamedPipe(pipe);
                        CloseHandle(pipe);
                    });
                } else {
                    CloseHandle(pipe);
                }
            }
        }
    }
}

/// Intenta conectar al servidor existente enviando `cmd`.
/// Si conecta, devuelve Secondary (quien llama debe salir).
/// Si no hay servidor, crea uno y devuelve Primary (quien llama lo ejecuta).
pub fn connect_or_serve(cmd: super::IpcCommand) -> Result<super::IpcConnection, Box<dyn std::error::Error>> {
    // Intentar conectar como cliente a un servidor existente.
    // Reintenta hasta 10 veces con 50 ms de espera para manejar el caso
    // donde el servidor aún está iniciando (evita race condition).
    for _ in 0..10 {
        unsafe {
            let pipe = CreateFileW(
                to_wide(PIPE_NAME).as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );

            if pipe != INVALID_HANDLE_VALUE {
                let msg = format!("{}\n", cmd.as_str());
                let mut bytes_written: u32 = 0;
                WriteFile(
                    pipe,
                    msg.as_ptr() as *const _,
                    msg.len() as u32,
                    &mut bytes_written,
                    std::ptr::null_mut(),
                );
                CloseHandle(pipe);
                return Ok(super::IpcConnection::Secondary);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // No hay servidor: esta es la primera instancia
    let server = WindowsIpcServer;
    Ok(super::IpcConnection::Primary(Box::new(server)))
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_wide_returns_null_terminated() {
        let wide = to_wide("test");
        assert_eq!(wide.last(), Some(&0));
        assert!(!wide.is_empty());
    }
}
