/// This module provides functionality for controlling the Bittrade Websockets service through Unix domain sockets.
/// It includes commands for serving, adding pairs, adding keys, and removing keys, as well as functions for handling
/// these commands and interacting with the Unix domain socket.
///
/// # Enums
///
/// - `ControlCommand`: Represents the different control commands that can be sent to the service.
///
/// # Functions
///
/// - `get_socket_path`: Retrieves the socket path from the environment variable `BITTRADE_SOCKET_PATH` or defaults to a predefined path.
/// - `get_stream`: Connects to the Unix domain socket and returns a `UnixStream`.
/// - `get_listener`: Binds a Unix domain socket listener to the socket path.
/// - `parse_control_command`: Parses a control command from a Unix stream.
/// - `prompt`: Prompts the user for a control command and returns the corresponding `ControlCommand`.
/// - `send_to_socket`: Sends a serialized control command to the Unix stream.
/// - `listen_to_control`: Listens for incoming control commands on the Unix domain socket and handles them accordingly.
///
/// # Errors
///
/// - `ControlError`: Represents errors that can occur while handling control commands.
///
/// # Usage
///
/// This module is intended to be used as part of the Bittrade Websockets service to allow for dynamic control
/// through Unix domain sockets. It provides a way to send commands such as adding trading pairs, adding API keys,
/// and removing API keys to the service.
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{io::{self, AsyncReadExt, AsyncWriteExt}, net::UnixListener, sync::{broadcast, mpsc}};
use cliclack::{input, select};
use tokio::net::UnixStream;
use bincode;

use crate::exchanges::exchange::{ExchangeName, BINANCE, BITFINEX, COINBASE, INDEPENDENT_RESERVE, KRAKEN, MEXC, WHITEBIT};
use std::env;

use super::errors::ControlError;

#[derive(Serialize, Deserialize, Debug)]
pub enum ControlCommand {
    Serve(String),
    AddPair(ExchangeName, String),             // Add pair command with an exchange and a single pair string
    AddKey(String, String),      // Add key command with two strings
    RemoveKey(String),           // Remove key command with one string
}

fn get_socket_path() -> String {
    let socket_path = env::var("BITTRADE_SOCKET_PATH").unwrap_or_else(|_| "/tmp/bittrade_websockets_control.sock".to_string());
    socket_path
}

async fn get_stream() -> Result<UnixStream, ControlError> {
    let path = get_socket_path();
    UnixStream::connect(path.clone()).await.map_err(|e| ControlError::SocketConnectionError(e, path))
}

pub async fn get_listener() -> Result<UnixListener, std::io::Error> {
    let socket_path = get_socket_path();
    if std::path::Path::new(&socket_path).exists() {
        tokio::fs::remove_file(&socket_path).await?;
    }
    UnixListener::bind(socket_path)
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


pub async fn prompt() -> Result<ControlCommand, ControlError> {
    loop {
        // After first loop where we attempt to get the command from the command line arguments, we will clear "args" and just keep prompting the user, unless this is a serve command
        let mut args: Vec<String> = env::args().collect();
        let action = match args.get(1) {
            Some(action) => action.to_owned(),
            None => {
                select("Select action:")
                .items(&[("serve", "Serve", ""), ("add_pair", "Add pair", ""), ("add_key", "Add key", ""), ("remove_key", "Remove key", "")])
                .initial_value("add_pair")
                .interact()?.into()
            }
        };
        // Serve action is final, we don't need to prompt the user for any more commands
        if action == "serve" {
            return Ok(ControlCommand::Serve(get_socket_path()));
        }
        // Clear the args vector so we can prompt the user for the action and arguments next time
        args = Vec::new();
        let action = action.as_str();
        let mut stream = get_stream().await?;
        match action {
            "add_pair" => {
                let exchange: &str = if args.len() > 1 {
                    let ex: &str = &args[2];
                    match ex {
                        BINANCE | WHITEBIT | KRAKEN | BITFINEX | MEXC | COINBASE | INDEPENDENT_RESERVE => {
                            ex
                        }
                        _ => {
                            log::error!("Invalid exchange: {}", ex);
                            continue;
                        }
                    }
                } else {
                    select("Select exchange:")
                    .items(&[(BINANCE, "Binance", ""), (WHITEBIT, "Whitebit", ""), (KRAKEN, "Kraken", ""), (BITFINEX, "Bitfinex", ""), (MEXC, "Mexc", ""), (COINBASE, "Coinbase", ""), (INDEPENDENT_RESERVE, "Independent Reserve", "")])
                    .initial_value(BINANCE)
                    .interact()?.into()
                };
                let pair = if args.len() > 2 {
                    args[3].to_owned()
                } else {
                    input("Enter pair:")
                    .default_input("BTC_USDT")
                    .validate(|input: &String| if input.is_empty() { Err("Please provide pair.") } else if input.len() == 7 || input.len() == 8 && input.chars().nth(3) == Some('/') && input.split('_').all(|part| part.len() == 3 || part.len() == 4) {
                        Ok(())
                    } else {
                        Err("Please provide a valid pair in the form BTC_USDT (all caps, with slash in the middle).")
                    })
                    .interact()?
                };
                
                // Send on the pairs channel
                let command = ControlCommand::AddPair(exchange.into(), pair);
                if let Err(err) = send_to_socket(&command, &mut stream).await.map(|_| command) {
                    log::error!("Failed to send command: {}", err);
                }
            }
            "add_key" => {
                log::error!("Add key not implemented yet");
            }
            "remove_key" => {
                log::error!("Remove key not implemented yet");
            }
            _ => {
                log::error!("Unknown action: {}", action);
            }
        }
    }
}

async fn send_to_socket(command: &ControlCommand, stream: &mut UnixStream) -> Result<(), ControlError> {
    let serialized_command = bincode::serialize(command).expect("Failed to serialize");

    // Send the length of the serialized command first
    let len = serialized_command.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;

    // Send the serialized data
    stream.write_all(&serialized_command).await?;

    Ok(())
}



pub async fn listen_to_control(pairs_sender: mpsc::Sender<(ExchangeName, String)>, account_sender: mpsc::Sender<String>) -> Result<(), ControlError> {
    let listener = get_listener().await?;

    // NOTE or TODO: This means we block to a single client connection? And each client connection can only send a single command?
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                match parse_control_command(stream).await {
                    Ok(command) => {
                        log::info!("Received command: {:?}", command);
                        match command {
                            ControlCommand::AddPair(exchange, pair) => {
                                pairs_sender.send((exchange, pair)).await?;
                            }
                            ControlCommand::AddKey(pair, key) => {
                                log::info!("Adding key: {} to pair: {}", key, pair);
                                account_sender.send(format!("{}:{}", pair, key)).await?;
                            }
                            ControlCommand::RemoveKey(key) => {
                                log::info!("Removing key: {}", key);
                                account_sender.send(key).await?;
                            }
                            _ => {
                                log::debug!("Command does not require action: {:?}", command);
                            }
                        }
                    }
                    Err(err) => {
                        log::error!("Error parsing command: {}", err);
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to accept connection: {}", e);
            }
        }
    }
}