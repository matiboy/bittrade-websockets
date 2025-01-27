use thiserror::Error;
use tokio::io;

use crate::control::ControlCommand;


#[derive(Error, Debug)]
pub enum MainError {
    #[error("IO error occurred: {0}")]
    IoError(#[from] io::Error),

    #[error("Failed to send on Control command channel")]
    SendErrorControlCommand(#[from ] tokio::sync::mpsc::error::SendError<ControlCommand>),
}