use std::{collections::HashMap, time::Duration};

use tokio::{select, sync::{broadcast, mpsc, oneshot}, time::sleep};

use crate::unix_socket::{create_socket, UnixSocketError};

use super::{binance::BinanceExchange, exchange::{ExchangeHandler, ExchangeName}};


#[derive(Debug)]
pub struct ExchangeManager {
    binance: BinanceExchange,
    price_sockets: HashMap<(ExchangeName, String), ExchangeHandler>,
}

impl ExchangeManager {
    pub fn new() -> Self {
        Self {
            binance: BinanceExchange::new(),
            price_sockets: HashMap::new(),
        }
    }

    pub async fn run(&mut self, mut pairs_receiver: mpsc::Receiver<(ExchangeName, String)>) {
        log::info!("Starting manager");
        // TODO Here we're using a select! because we will be expecting to receive api keys and pairs from the control channel
        select! {
            _ = async {
                loop {
                    if let Some((exchange, pair)) = pairs_receiver.recv().await {
                        log::info!("Received pair for exchange: {exchange} {pair}");
                        self.add_pair(exchange, pair).await;
                    }
                }
            } => {
                log::error!("Pair receiver terminated");
            }
        }
    }

    pub async fn add_pair(&mut self, exchange_name: ExchangeName, pair: String) {
        if self.price_sockets.contains_key(&(exchange_name.clone(), pair.clone())) {
            log::warn!("Pair {pair} already exists for exchange {exchange_name}");
            return;
        }
        // Initially the exchanges are None and we only create them when we need to add a pair
        // TODO Is this the right way to look at it? Should we create the exchanges when we start the manager? And they can decide for themselves whether to run their websocket(s) or not
        // TODO Binance currently can return the matching pair but other exchanges might not be so simple
        // Create a channel that will be dedicated to this pair
        let (messages_sender, messages_receiver) = mpsc::channel(1024);
        let path = get_socket_name(&exchange_name, &pair);
        let handler = ExchangeHandler::bridge(path, messages_receiver).await;
        self.price_sockets.insert((exchange_name.clone(), pair.clone()), handler);
        match exchange_name {
            ExchangeName::Binance => {
                self.binance.add_pair(&pair, messages_sender).await;
            }
            _ => {
                log::error!("Exchange not implemented");
            }
        };
        
        
        
    }

    async fn remove_pair(&mut self, exchange_name: ExchangeName, pair: String) {
        
    }
}


fn get_socket_name(exchange_name: &ExchangeName, pair: &String) -> String {
    let base_folder = std::env::var("UNIX_SOCKET_BASE_PATH")
    .map_or_else(|_| "/tmp/".to_owned(), |s| if s.ends_with('/') { s } else { format!("{}/", s) });
    format!("{base_folder}{exchange_name}_{pair}.sock")
}
