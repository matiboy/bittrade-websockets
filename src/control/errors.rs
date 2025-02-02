use thiserror::Error;
use tokio::{io, sync::{broadcast, mpsc}};

use crate::exchanges::exchange::ExchangeName;

use super::control::ControlCommand;


#[derive(Error, Debug)]
pub enum ControlError {
    #[error("IO error occurred: {0}")]
    Io(#[from] io::Error),

    #[error("Channel send error: {0}")]
    SendErrorBroadcast(#[from] broadcast::error::SendError<String>),
    
    #[error("Channel send error: {0}")]
    SendErrorMpsc(#[from] mpsc::error::SendError<String>),
    
    #[error("Channel send error: {0}")]
    SendPairErrorMpsc(#[from] mpsc::error::SendError<(ExchangeName, String)>),

    #[error("Failed to send on Control command channel")]
    SendErrorControlCommand(#[from] mpsc::error::SendError<ControlCommand>),

    #[error("Failed to connect to socket: {1}; IO error: {0}")]
    SocketConnectionError(io::Error, String),

    #[error("Unexpected control listener loop termination: {0}")]
    ListenToControlError(String),
}
