use thiserror::Error;
use tokio::io;


#[derive(Error, Debug)]
pub enum MainError {
    #[error("IO error occurred: {0}")]
    IoError(#[from] io::Error),

    
}