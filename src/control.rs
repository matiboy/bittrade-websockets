use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader}, net::UnixListener, stream, sync::{broadcast, mpsc}};
use cliclack::{password, input, select};
use tokio::net::UnixStream;
use bincode;

const SOCKET_PATH: &str = "/tmp/tokio_unix_socket";

#[derive(Serialize, Deserialize, Debug)]
pub enum ControlCommand {
    AddPair(String),             // Add pair command with a single string
    AddKey(String, String),      // Add key command with two strings
    RemoveKey(String),           // Remove key command with one string
}

#[derive(Error, Debug)]
pub enum ControlError {
    #[error("IO error occurred: {0}")]
    Io(#[from] io::Error),

    #[error("Channel send error: {0}")]
    SendErrorBroadcast(#[from] broadcast::error::SendError<String>),
    
    
    #[error("Channel send error: {0}")]
    SendErrorMpsc(#[from] mpsc::error::SendError<String>),

}

async fn get_stream() -> Result<UnixStream, std::io::Error> {
    UnixStream::connect(SOCKET_PATH).await
}

pub async fn get_listener() -> Result<UnixListener, std::io::Error> {
    if std::path::Path::new(SOCKET_PATH).exists() {
        tokio::fs::remove_file(SOCKET_PATH).await?;
    }
    UnixListener::bind(SOCKET_PATH)
}

pub async fn parse_control_command(mut stream: UnixStream) -> Result<ControlCommand, ControlError> {
    // Read the length of the incoming message (first 4 bytes)
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    // Read the serialized command data
    let mut buf = vec![0; len];
    stream.read_exact(&mut buf).await?;

    // Deserialize the command using bincode
    let command: ControlCommand = bincode::deserialize(&buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(command)
}

pub async fn prompt() -> Result<(), ControlError> {
    let action: &str = select("Select action:")
        .items(&[("add_pair", "Add pair", ""), ("add_key", "Add key", ""), ("remove_key", "Remove key", "")])
        .initial_value("add_pair")
        .interact()?;
    dbg!(&action);
    let mut stream = get_stream().await?;
    dbg!(&action);
    match action {
        "add_pair" => {
            dbg!("Add pair");
            let pair: String = input("Enter pair:")
                .placeholder("BTC/USDT")
                .validate(|input: &String| if input.is_empty() { Err("Please provide pair.") } else if (input.len() == 7 || input.len() == 8 && input.chars().nth(3) == Some('/') && input.split('/').all(|part| part.len() == 3 || part.len() == 4)) {
                    Ok(())
                } else {
                    Err("Please provide a valid pair in the form BTC/USDT (all caps, with slash in the middle).")
                })
                .interact()?;
            // Send on the pairs channel
            dbg!(&pair);
            return send_to_socket(ControlCommand::AddPair(pair), &mut stream).await;
        }
        "add_key" => {
            println!("Add key");
        }
        "remove_key" => {
            println!("Remove key");
        }
        _ => {
            eprintln!("Unknown action: {}", action);
        }
    }

    let api_key: String = input("Enter API Key:")
        .placeholder("Your API key here")
        .validate(|input: &String| if input.is_empty() { Err("Please provide API key.") } else  { Ok(()) })
        .interact()?;

    let password: String = password("Enter Password:")
        .validate(|input: &String| if input.is_empty() { Err("Please provide API key.") } else  { Ok(()) })
        .interact()?;

    let mut stream = UnixStream::connect(SOCKET_PATH).await?;

    let credentials = format!("{}:{}", api_key, password);
    stream.write_all(credentials.as_bytes()).await?;
    stream.flush().await?;

    Ok(())
}

async fn send_to_socket(command: ControlCommand, stream: &mut UnixStream) -> Result<(), ControlError> {
    let serialized_command = bincode::serialize(&command).expect("Failed to serialize");

    // Send the length of the serialized command first
    let len = serialized_command.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;

    // Send the serialized data
    stream.write_all(&serialized_command).await?;

    Ok(())
}

pub async fn control_dispatcher(mut control_receiver: mpsc::Receiver<ControlCommand>, pairs_sender: mpsc::Sender<String>, account_sender: mpsc::Sender<String>) -> Result<(), ControlError> {
    loop {
        if let Some(command) = control_receiver.recv().await {
            match command {
                ControlCommand::AddPair(pair) => {
                    pairs_sender.send(pair).await?;
                }
                ControlCommand::AddKey(pair, key) => {
                    println!("Adding key: {} to pair: {}", key, pair);
                    account_sender.send(format!("{}:{}", pair, key)).await?;
                }
                ControlCommand::RemoveKey(key) => {
                    println!("Removing key: {}", key);
                    account_sender.send(key).await?;
                }
            }
        }
    }
}