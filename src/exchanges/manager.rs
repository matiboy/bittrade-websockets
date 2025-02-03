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
        // TODO Looks like we are keeping the receiver around for the lifetime of the manager, just so it doesn't get dropped while there is no other receiver (before pairs get added); is this the right way to do it?
        Self {
            binance: None,
            from_exchanges_pair_price_sender,
            from_exchanges_pair_price_receiver,
            unix_sockets: HashMap::new(),
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
        if self.unix_sockets.contains_key(&(exchange_name.clone(), pair.clone())) {
            log::warn!("Pair {pair} already exists for exchange {exchange_name}");
            return;
        }
        // Initially the exchanges are None and we only create them when we need to add a pair
        // TODO Is this the right way to look at it? Should we create the exchanges when we start the manager? And they can decide for themselves whether to run their websocket(s) or not
        // TODO Binance currently can return the matching pair but other exchanges might not be so simple
        let matching_pair = match exchange_name {
            ExchangeName::Binance => {
                let exchange = self.binance.get_or_insert_with(|| BinanceExchange::new(self.from_exchanges_pair_price_sender.clone()));
                // We want to know what pair will actually be sent by the exchange as it won't be the same as the one we requested e.g. BTC_USDT will become BTCUSDT
                exchange.add_pair(&pair).await
            }
            _ => {
                log::error!("Exchange not implemented");
                "".to_owned()
            }
        };
        log::info!("Added pair {matching_pair} for exchange {exchange_name}");
        
        // TODO we need to wrap this in a loop so that if either the channel or the unix listener dies, we can restart it
        // but for now, focus on getting something running
        let (stop_sender, stop_receiver) = oneshot::channel();
        
        let handler = ExchangePairHandler {
            stop_channel: stop_sender,
        };
        self.unix_sockets.insert((exchange_name.clone(), pair.clone()), handler);
        let mut manager_receiver = self.from_exchanges_pair_price_sender.subscribe();
        // TODO we should keep this task handler around so we can abort it when needed
        tokio::spawn(async move {
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
