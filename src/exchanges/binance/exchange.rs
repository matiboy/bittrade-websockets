use tokio::{select, sync::{broadcast, mpsc}};

use crate::exchanges::{binance::websocket::public::websocket_connection, exchange::BINANCE, messages::ExchangePairPrice};

#[derive(Debug)]
pub struct BinanceExchange {
    public_websocket_handle: tokio::task::JoinHandle<()>,
    new_pairs_sender: mpsc::Sender<String>,
}

impl BinanceExchange {
    pub fn new(to_manager_sender: broadcast::Sender<ExchangePairPrice>) -> Self {
        let (new_pairs_sender, new_pairs_receiver) = mpsc::channel(32);
        let (websocket_to_manager_bridge_sender, mut websocket_to_manager_bridge_receiver) = mpsc::channel(128);
        let public_websocket_handle = tokio::spawn(async move {
            log::info!("Public websocket handle spawned for Binance");
            let public_url = 
            std::env::var("BINANCE_PUBLIC_WS_URL").unwrap_or_else(|_| get_default_public_url());
            select! {
                _ = websocket_connection(&public_url, new_pairs_receiver, websocket_to_manager_bridge_sender) => {
                    log::error!("Websocket connection terminated");
                }
                _ = async {
                    loop {
                        if let Some(message) = websocket_to_manager_bridge_receiver.recv().await {
                            if let Err(err) = to_manager_sender.send(ExchangePairPrice {
                                exchange: BINANCE.into(),
                                pair: message.pair,
                                ask: message.ask,
                                bid: message.bid,
                            }) {
                                log::error!("Failed to send message to manager: {}", err);
                            }
                        }
                    }
                } => {
                    log::error!("Websocket to manager bridge terminated");
                }
            }
        });
        Self {
            public_websocket_handle,
            new_pairs_sender,
        }
    }

    pub async fn add_pair(&mut self, pair: &String) -> String {
        let pair = pair.replace("_", "");
        if let Err(err) = self.new_pairs_sender.send(pair.clone()).await {
            log::error!("Failed to send pair to websocket: {}", err);
        }
        pair
    }

}

impl Drop for BinanceExchange {
    fn drop(&mut self) {
        log::info!("Dropping Binance exchange");
        self.public_websocket_handle.abort();
    }
}


pub fn get_default_public_url() -> String {
    "wss://data-stream.binance.vision/ws".to_owned()
}
pub fn get_default_private_url() -> String {
    "wss://data-stream.binance.vision/ws".to_owned()
}
