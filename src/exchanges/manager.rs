use std::{collections::HashMap, env, time::Duration};

use tokio::{select, sync::{broadcast, mpsc, oneshot}, time::sleep};

use crate::unix_socket::{create_socket, UnixSocketError};

use super::{exchange::{ExchangeName, GenericExchange, HasPairs}, messages::ExchangePairPrice};
use super::binance::BinanceExchange;

#[derive(Debug)]
struct ExchangePairHandler {
    stop_channel: oneshot::Sender<()>,
}

#[derive(Debug)]
pub struct ExchangeManager {
    binance: Option<GenericExchange<BinanceExchange>>,
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

    // fn get_matching_pair(&self, pair: &str) -> String {
    //     match self.exchange {
    //         Exchange::Binance => pair.to_uppercase().replace("/", "").replace("_", ""),
    //         _ => unimplemented!(),
    //     }
    // }
    
    fn get_exchange(&self, exchange: &ExchangeName) -> Option<&GenericExchange<BinanceExchange>> {
        match exchange {
            ExchangeName::Binance => self.binance.as_ref(),
            _ => unimplemented!(),
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
                        log::info!("Received pair: {}", pair);
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
        let mut exchange = self.get_exchange(&exchange_name);
        // let exchange: &mut GenericExchange<_> = exchange.get_or_insert(GenericExchange::new(BinanceExchange::new()));
        // exchange.add_pair(&pair).await;
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
                                log::info!("Received pair price: {:?}", pair_price);
                                tx.send(format!("{},{}", pair_price.ask, pair_price.bid)).expect("Failed to send message");
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
    let base_folder = std::env::var("UNIX_SOCKET_BASE_PATH").unwrap_or_else(|_| env::temp_dir().to_str().unwrap().to_owned());
    format!("{}/{}_{}.sock", base_folder, String::from(exchange_name), pair)
}

async fn create_unix_socket_listener(path: &String, stop: oneshot::Receiver<()>, tx: &broadcast::Sender<String>, rx: &mut broadcast::Receiver<String>) {
    select! {
        _ = stop => {
            log::info!("Received stop signal");
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
