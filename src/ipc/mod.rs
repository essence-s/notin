//! IPC entre instancias de notin.
//! La primera instancia actúa como servidor (escucha en socket/named pipe).
//! Las segundas instancias actúan como clientes: envían el comando y salen.
//!
//! En Linux se usa Unix domain socket.
//! En Windows se usa named pipe.

/// Comando que una instancia secundaria envía a la primera: abrir la ventana
/// flotante de dictado (`NotinWindow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcCommand {
    /// Abre (o alterna) la ventana flotante de dictado.
    OpenNotin,
}

impl IpcCommand {
    /// Representación enviada por el socket/named pipe al servidor.
    fn as_str(self) -> &'static str {
        match self {
            IpcCommand::OpenNotin => "open-notin",
        }
    }
}

/// Resultado de intentar conectar al servidor existente.
pub enum IpcConnection {
    /// Es la primera instancia (servidor).
    Primary(Box<dyn IpcServer + Send>),
    /// Es una instancia secundaria: envió el comando y debe salir.
    Secondary,
}

/// Trait implementado por cada plataforma para el servidor de IPC.
pub trait IpcServer: Send {
    /// Escucha comandos entrantes y ejecuta el callback para cada uno.
    fn run(&self, on_command: Box<dyn Fn(IpcCommand) + Send + Sync + 'static>);
}

/// Intenta conectar al servidor existente enviando `cmd`.
/// Si conecta, devuelve Secondary (quien llama debe salir).
/// Si no hay servidor, crea uno y devuelve Primary (quien llama lo ejecuta).
pub fn connect_or_serve(cmd: IpcCommand) -> Result<IpcConnection, Box<dyn std::error::Error>> {
    platform::connect_or_serve(cmd)
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "windows")]
use windows as platform;
