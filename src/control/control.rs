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
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::UnixListener, select, sync::{mpsc, Mutex}};
use cliclack::{input, select};
use tokio::net::UnixStream;
use bincode;

use crate::exchanges::exchange::{ExchangeName, BINANCE, BITFINEX, COINBASE, INDEPENDENT_RESERVE, KRAKEN, MEXC, WHITEBIT};
use std::{env, sync::Arc};

use super::errors::ControlError;


pub async fn listen_to_control(pairs_sender: mpsc::Sender<(ExchangeName, String)>, account_sender: mpsc::Sender<String>) -> Result<(), ControlError> {
    let (fatal_sender, mut fatal_receiver) = mpsc::channel::<ControlError>(1);
    select! {
        // The regular control connection loop; not expected to return
        _ = async {
            // TODO: Handle listener error properly here
            let listener = get_listener().await.expect("Failed to get listener");
            loop {
                if let Err(err) = control_connection(&listener, pairs_sender.clone(), account_sender.clone(), fatal_sender.clone()).await {
                    log::error!("Control connection error {}", err);
                }
            }
        } => {
            log::error!("Control connection loop died");
            Err(ControlError::ListenToControlError("Control connection loop died".to_owned()))
        },
        // In case of a fatal error, this will break the select! and thus propagate to the main function
        Some(err) = fatal_receiver.recv() => {
            log::error!("Fatal error in control listener: {}", err);
            Err(ControlError::ListenToControlError(err.to_string()))
        }
    }
}

async fn control_connection(listener: &UnixListener, pairs_sender: mpsc::Sender<(ExchangeName, String)>, _account_sender: mpsc::Sender<String>, outcome_sender: mpsc::Sender<ControlError>) -> Result<(), ControlError> {
    let (stream, _) = listener.accept().await?;
    // tokio::spawn requires 'static lifetime, so we need to clone the stream and wrap it in Arc<Mutex>
    let stream = Arc::new(Mutex::new(stream));
    // We need to spawn so that we don't block other incoming connections
    tokio::spawn(async move {
    loop {
        // TODO we still seem to be dropping the connection (see log showing IO error)
        // NOTE on the above; it's actually just the get_or_insert which isn't lazy so it's creating a new stream every time and dropping it immediately
        let stream = stream.clone();
        match parse_control_command(stream).await {
            Ok(command) => {
                log::info!("Received command: {:?}", command);
                match command {
                    ControlCommand::AddPair(exchange, pair) => {
                        if let Err(err) = pairs_sender.send((exchange, pair)).await {
                            log::error!("Failed to send pair: {}", err);
                            // At this stage, our internal channel for adding pairs has failed so if the outcome sender which bubbles that information also fails, we're in so much trouble that using expect seems fine.
                            outcome_sender.send(ControlError::SendPairErrorMpsc(err)).await.expect("Failed to send error");
                        }
                    }
                    ControlCommand::AddKey(pair, key) => {
                        log::info!("Adding key: {} to pair: {}", key, pair);
                        // account_sender.send(format!("{}:{}", pair, key)).await?;
                    }
                    ControlCommand::RemoveKey(key) => {
                        log::info!("Removing key: {}", key);
                        // account_sender.send(key).await?;
                    }
                }
            }
            Err(ControlError::Io(err)) => {
                log::warn!("IO error: {}; broken connection", err);
                break;
            }
            Err(err) => {
                log::warn!("Error parsing command: {}", err);
            }
        }
    }
    });
    Ok(())
}

const PAIR_SEPARATOR: char = '_';

pub async fn prompt() -> Result<PromptResult, ControlError> {
    // After first loop where we attempt to get the command from the command line arguments, we will clear "args" and just keep prompting the user, unless this is a serve command
    let mut args: Vec<String> = env::args().collect();
    // After first loop we will also stop offering "serve" as an option
    let mut action_options = [("serve", "Serve", ""), ("add_pair", "Add pair", ""), ("add_key", "Add key", ""), ("remove_key", "Remove key", "")].to_vec();
    let mut is_first_loop = true;
    let mut stream = None;
    loop {
        let action = match args.get(1) {
            Some(action) => action.to_owned(),
            None => {
                select("Select action:")
                .items(&action_options)
                .initial_value("add_pair")
                .interact()?.into()
            }
        };
        // Serve action is final, we don't need to prompt the user for any more commands
        if action == "serve" {
            return Ok(PromptResult::Serve(get_socket_path()));
        }
        if stream.is_none() {
            stream = Some(get_stream().await?);
        }
        let stream = stream.as_mut().unwrap();
        // Clear the args vector so we can prompt the user for the action and arguments next time
        args = Vec::new();
        // Also remove the serve option from the list of options
        if is_first_loop {
            action_options.retain(|(key, _, _)| *key != "serve");
            is_first_loop = false;
        }
        match action.as_str() {
        "add_pair" => {
            let exchange: &str = if args.len() > 1 {
                let ex: &str = args[2].as_str();
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
                .validate(|input: &String| if input.is_empty() { Err("Please provide pair.".to_owned()) } else if !input.contains(PAIR_SEPARATOR) {
                    let message = format!("Pair must contain separator. {}", PAIR_SEPARATOR);
                    Err(message)
                } else {
                    Ok(())
                })
                .interact()?
            };
            
            // Send on the pairs channel
            let command = ControlCommand::AddPair(exchange.into(), pair);
            if let Err(err) = send_to_socket(&command, stream).await.map(|_| command) {
                log::error!("Failed to send command: {}", err);
                return Err(ControlError::ListenToControlError(err.to_string()));
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


#[derive(Serialize, Deserialize, Debug)]
pub enum ControlCommand {
    AddPair(ExchangeName, String),             // Add pair command with an exchange and a single pair string
    AddKey(String, String),      // Add key command with two strings
    RemoveKey(String),           // Remove key command with one string
}

pub enum PromptResult {
    Serve(String),
    Exit,
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


async fn send_to_socket(command: &ControlCommand, stream: &mut UnixStream) -> Result<(), ControlError> {
    let serialized_command = bincode::serialize(command).expect("Failed to serialize");

    // Send the length of the serialized command first
    let len = serialized_command.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;

    // Send the serialized data
    stream.write_all(&serialized_command).await?;

    Ok(())
}



pub async fn parse_control_command(stream: Arc<Mutex<UnixStream>>) -> Result<ControlCommand, ControlError> {
    // Read the length of the incoming message (first 4 bytes)
    let mut len_buf = [0u8; 4];
    let mut stream = stream.lock().await;
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
