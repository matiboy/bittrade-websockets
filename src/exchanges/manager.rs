use std::{collections::HashMap, env, time::Duration};

use tokio::{select, sync::{broadcast, mpsc, oneshot}, time::sleep};

use crate::unix_socket::{create_socket, UnixSocketError};

use super::{exchange::ExchangeName, messages::ExchangePairPrice};
use super::binance::BinanceExchange;

#[derive(Debug)]
struct ExchangePairHandler {
    stop_channel: oneshot::Sender<()>,
}

#[derive(Debug)]
pub struct ExchangeManager {
    binance: Option<BinanceExchange>,
    from_exchanges_pair_price_sender: broadcast::Sender<ExchangePairPrice>,
    from_exchanges_pair_price_receiver: broadcast::Receiver<ExchangePairPrice>,
    unix_sockets: HashMap<(ExchangeName, String), ExchangePairHandler>,
}

impl ExchangeManager {
    pub fn new() -> Self {
        let (from_exchanges_pair_price_sender, from_exchanges_pair_price_receiver) = broadcast::channel(1024);
        Self {
            binance: None,
            from_exchanges_pair_price_sender,
            from_exchanges_pair_price_receiver,
            unix_sockets: HashMap::new(),
        }
    }

    pub async fn run(&mut self, mut pairs_receiver: mpsc::Receiver<(ExchangeName, String)>) {
        // Temporarily sending fake messages to check whether our Unix socket side works
        let sender = self.from_exchanges_pair_price_sender.clone();
        log::info!("Starting manager");
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
            _ = async {
                #[allow(unused_variables)]  // This is a temporary loop to send fake messages
                let mut i = 0.; 
                loop {
                    sleep(Duration::from_secs(1)).await;
                    i += 0.1;
                    // sender.send(ExchangePairPrice {
                    //     exchange: ExchangeName::Binance,
                    //     pair: "XRP_USDT".to_owned(),
                    //     ask: i,
                    //     bid: 0.2,
                    // }).expect("Failed to send message");
                }
            } => {
                log::error!("Pair price sender terminated");
            }
        }
        
    }

    pub async fn add_pair(&mut self, exchange_name: ExchangeName, pair: String) {
        if self.unix_sockets.contains_key(&(exchange_name.clone(), pair.clone())) {
            return;
        }
        // Initially the exchanges are None and we only create them when we need to add a pair
        // TODO Is this the right way to look at it? Should we create the exchanges when we start the manager? And they can decide for themselves whether to run their websocket(s) or not
        match exchange_name {
            ExchangeName::Binance => {
                let exchange = self.binance.get_or_insert(BinanceExchange::new(self.from_exchanges_pair_price_sender.clone()));
                exchange.add_pair(&pair).await;
            }
            _ => {
                log::error!("Exchange not implemented");
            }
        }
        
        // TODO we need to wrap this in a loop so that if either the channel or the unix listener dies, we can restart it
        // but for now, focus on getting something running
        let (stop_sender, stop_receiver) = oneshot::channel();
        
        let handler = ExchangePairHandler {
            stop_channel: stop_sender,
        };
        self.unix_sockets.insert((exchange_name.clone(), pair.clone()), handler);
        let mut manager_receiver = self.from_exchanges_pair_price_sender.subscribe();
        tokio::spawn(async move {
            let path = get_socket_name(&exchange_name, &pair);
            let (tx, mut rx) = broadcast::channel::<String>(32);
            select! {
                _ = create_unix_socket_listener(&path, stop_receiver, &tx, &mut rx) => {
                    log::info!("Unix socket listener died");
                }
                _ = async {
                    loop {
                        if let Ok(pair_price) = manager_receiver.recv().await {
                            if pair_price.exchange == exchange_name && pair_price.pair == pair {
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
        });
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
