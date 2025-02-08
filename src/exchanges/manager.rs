use std::{collections::HashMap, time::Duration};

use tokio::{select, sync::{broadcast, mpsc, oneshot}, task::JoinHandle, time::sleep};

use crate::unix_socket::{create_socket, UnixSocketError};

use super::{binance::BinanceExchange, exchange::{ExchangeHandler, ExchangeName}, messages::ExchangePairPrice};


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
        let (messages_sender, messages_receiver) = broadcast::channel(1024);
        match exchange_name {
            ExchangeName::Binance => {
                self.binance.add_pair(&pair, messages_sender).await;
            }
            _ => {
                log::error!("Exchange not implemented");
            }
        };
        log::info!("Added pair {matching_pair} for exchange {exchange_name}");
        let (stop_sender, stop_receiver) = oneshot::channel();
        let handler = ExchangeHander::bridge(messages_receiver, stop_sender, stop_receiver);
        
        // TODO we need to wrap this in a loop so that if either the channel or the unix listener dies, we can restart it
        // but for now, focus on getting something running
        let mut manager_receiver = self.from_exchanges_pair_price_sender.subscribe();
        let (pair_clone, exchange_clone) = (pair.clone(), exchange_name.clone());
        let handler = ExchangePairHandler {
            stop_channel: stop_sender,
            task: tokio::spawn(async move {
                let path = get_socket_name(&exchange_name, &pair);
                let (tx, mut rx) = broadcast::channel::<String>(32);
                select! {
                    _ = create_unix_socket_listener(&path, stop_receiver, &tx, &mut rx) => {
                        log::info!("Unix socket listener died");
                    }
                    _ = async {
                        let mut last_value = (0., 0.);
                        loop {
                            if let Ok(pair_price) = manager_receiver.recv().await {
                                // We match on the pair sent by the exchange, ...
                                if pair_price.exchange == exchange_name && pair_price.pair == matching_pair && (pair_price.ask, pair_price.bid) != last_value {
                                    last_value = (pair_price.ask, pair_price.bid);
                                    // ... but then we modify it back to what we had requested
                                    let mut pair_price = pair_price;
                                    pair_price.pair = pair.clone();
                                    log::debug!("Received pair price: {:?}", pair_price);
                                    // TODO we might need to differentiate between the different types of errors, failing to serialize is not the same issue as failing to send on the channel
                                    if let Err(err) = serde_json::to_string(&pair_price).map(|s| tx.send(s + "\n")) {
                                        log::warn!("Failed to serialize pair price: {err}");
                                    }
                                }
                            }   
                        }
                    } => {
                        log::info!("Manager receiver died");
                    }
                }
            })
        };
        self.unix_sockets.insert((exchange_clone, pair_clone), handler);
        
    }

    async fn remove_pair(&mut self, exchange_name: ExchangeName, pair: String) {
        
    }
}


fn get_socket_name(exchange_name: &ExchangeName, pair: &String) -> String {
    let base_folder = std::env::var("UNIX_SOCKET_BASE_PATH")
    .map_or_else(|_| "/tmp/".to_owned(), |s| if s.ends_with('/') { s } else { format!("{}/", s) });
    format!("{base_folder}{exchange_name}_{pair}.sock")
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
