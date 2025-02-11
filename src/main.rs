use std::fs::OpenOptions;

use exchanges::exchange::ExchangeName;
use runner::run;
use tokio::select;
use control::control::{listen_to_control, prompt, PromptResult};
use tokio::sync::mpsc;
use tracing::Level;
use tracing_subscriber::{
    prelude::*,
    fmt,
    layer::Layer,
    Registry, filter
};

mod control;
mod runner;
mod errors;
mod exchanges;
mod unix_socket;
mod websocket;
mod json;
mod channels;


#[tokio::main]
async fn main() {
    let debug_file = OpenOptions::new()
        .append(true)
        .create(true)
        .open("logs/log-debug.log")
        .unwrap();
    let subscriber = Registry::default()
        .with(
            // stdout layer, to view everything in the console
            fmt::layer()
                .compact()
                .with_ansi(true)
        )
        .with(
            // log-debug file, to log the debug
            fmt::layer()
                .with_ansi(false)
                .with_file(true)
                .with_line_number(true)
                .with_writer(debug_file)
                .with_filter(filter::LevelFilter::from_level(Level::DEBUG))
        );
    
    tracing::subscriber::set_global_default(subscriber).unwrap();

    // Entry point for the program - either read the command from the command line arguments or prompt the user
    match prompt().await {
        // Only the serve command requires the programme to continue
        Ok(PromptResult::Serve(path)) => {
            tracing::info!("Starting server at {}", path);
        },
        Ok(_) => {
            tracing::info!("Done");
            return;
        }
        Err(e) => {
            tracing::error!("Failed to get command: {}", e);
            return;
        }, 
    }

    // This is used to communicate to the main server that a new Exchange/Pair is expected to be added to the list of watched exchange/pairs
    let (pairs_sender, pairs_receiver) = mpsc::channel::<(ExchangeName, String)>(32);
    // This is used to communicate to the main server that a new account key is expected to be added to the list of watched account keys/secrets - Not implemented yet
    let (account_sender, _) = mpsc::channel::<String>(32);

    let manager_task = tokio::spawn(run(pairs_receiver));

    // For dev purposes
    // pairs_sender.send((ExchangeName::Binance, "XRP_USDT".to_owned())).await.expect("Failed to send pair");

    select! {
        // Listens to control commands sent via the unix domain socket like adding new pairs or account keys
        _ = listen_to_control(pairs_sender, account_sender) => {
            log::error!("Control listener failed");
        },
        // This is the main server that listens to the exchanges and sends the data to the unix sockets
        _ = manager_task => {
            log::error!("Websocket runner completed");
        },
    }
}
