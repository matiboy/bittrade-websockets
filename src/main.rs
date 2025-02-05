use exchanges::exchange::ExchangeName;
use runner::run;
use tokio::select;

mod control;
mod runner;
mod errors;
mod exchanges;
mod unix_socket;
mod websocket;
use control::control::{listen_to_control, prompt, PromptResult};
use tokio::sync::mpsc;


#[tokio::main]
async fn main() {
    // console_subscriber::init();
    env_logger::init();

    // Entry point for the program - either read the command from the command line arguments or prompt the user
    match prompt().await {
        // Only the serve command requires the programme to continue
        Ok(PromptResult::Serve(path)) => {
            log::info!("Starting server at {}", path);
        },
        Ok(_) => {
            log::info!("Done");
            return;
        }
        Err(e) => {
            log::error!("Failed to get command: {}", e);
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
