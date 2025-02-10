use std::time::Duration;

use tokio::{select, sync::{broadcast, mpsc, oneshot}, task::JoinHandle, time::sleep};

use crate::{exchanges::messages::ExchangePairPrice, unix_socket::{create_socket, UnixSocketError}};


#[derive(Debug)]
pub struct ExchangeHandler {
    pub stop_sender: oneshot::Sender<()>,
    pub task: JoinHandle<()>,
    pub sender: broadcast::Sender<ExchangePairPrice>,
}

impl ExchangeHandler {
    pub async fn bridge(path: String, mut manager_receiver: mpsc::Receiver<ExchangePairPrice>) -> Self {
        let (stop_sender, stop_receiver) = oneshot::channel();
        let (sender, receiver) = broadcast::channel(1024);
        Self {
            stop_sender,
            sender,
            task: tokio::spawn(async move {
                let (tx, mut rx) = broadcast::channel::<String>(32);
                select! {
                    _ = create_unix_socket_listener(&path, stop_receiver, &tx, &mut rx) => {
                        log::info!("Unix socket listener died");
                    }
                    _ = async {
                        let mut last_value = (0., 0.);
                        loop {
                            if let Some(pair_price) = manager_receiver.recv().await {
                                if (pair_price.ask, pair_price.bid) == last_value {
                                    continue;
                                }
                                last_value = (pair_price.ask, pair_price.bid);
                                if let Err(err) = serde_json::to_string(&pair_price).map(|s| tx.send(s + "\n")) {
                                    log::warn!("Failed to serialize pair price: {err}");
                                }
                            }   
                        }
                    } => {
                        log::info!("Manager receiver died");
                    }
                }
            })
        }
    }
}


async fn create_unix_socket_listener(path: &String, stop: oneshot::Receiver<()>, tx: &broadcast::Sender<String>, rx: &mut broadcast::Receiver<String>) {
    select! {
        _ = stop => {
            log::info!("Received stop signal for socket on path {path}");
        },
        _ = async {
            loop {
                match create_socket(path, tx, rx).await {
                    Ok(()) => {
                        log::info!("Socket listener completed due to stop signal");
                        break;
                    }
                    Err(UnixSocketError::Fatal(err)) => {
                        log::error!("Fatal error in socket listener: {}", err);
                        break;
                    }
                    Err(UnixSocketError::ConnectionError(err)) => {
                        log::error!("Connection error in socket listener: {}; retrying", err);
                        sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        } => {
            log::info!("Failed to receive messages from manager");
        }
    }
    
}
