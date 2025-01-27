use std::env;
use std::io::Error;
use tokio::select;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

mod control;
mod runner;
mod errors;
mod exchanges;
use runner::run_public_websocket;
use control::{control_dispatcher, parse_control_command, prompt, ControlCommand};
use tokio::sync::mpsc;


#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 1 {
        return prompt().await.expect("Failed to prompt");
    }

    let (control_sender, mut control_receiver) = mpsc::channel::<ControlCommand>(32);
    let (pairs_sender, pairs_receiver) = mpsc::channel::<String>(32);
    let (account_sender, account_receiver) = mpsc::channel::<String>(32);

    select! {
        _ = listen_to_control(control_sender) => {
            eprintln!("Control listener failed");
        },
        _ = control_dispatcher(control_receiver, pairs_sender, account_sender) => {
            eprintln!("Control dispatcher failed");
        },
        _ = run_public_websocket(pairs_receiver) => {
            eprintln!("Public websocket runner failed");
        },
    }
}

async fn listen_to_control(control_sender: mpsc::Sender<ControlCommand>) -> Result<(), errors::MainError> {
    let listener = control::get_listener().await.expect("Failed to get listener");

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                match parse_control_command(stream).await {
                    Ok(command) => {
                        println!("Received command: {:?}", command);
                        control_sender.send(command).await?;
                    }
                    Err(err) => {
                        eprintln!("Error parsing command: {}", err);
                    }
                }
                // handle_client(stream, &control_sender).await;
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }
}