mod errors;
mod deserializer;
use std::{collections::HashSet, sync::Arc, time::Duration};

use errors::WebsocketConnectionError;
use futures::{stream::SplitSink, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};


use tokio::{net::TcpStream, select, sync::{broadcast, mpsc, RwLock}, time::{self, sleep}};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::exchanges::{errors::ExchangeApiError, exchange::BINANCE, messages::ExchangePairPrice};
use deserializer::deserialize_f64_from_string;
// use super::exchange::{ExchangeName, ExchangeApi};


#[derive(Debug, Deserialize)]
struct BinancePairMessage {
    #[serde(rename = "s")]
    pair: String,
    #[serde(rename = "a", deserialize_with = "deserialize_f64_from_string")]
    ask: f64,
    #[serde(rename = "b", deserialize_with = "deserialize_f64_from_string")]
    bid: f64,
}


#[derive(Serialize)]
struct BinanceSubscriptionMessage {
    method: String,
    params: Vec<String>,
}

pub fn create_binance_subscription_message(pairs: &HashSet<String>) -> String {
    let channels = pairs.iter().map(|pair| format!("{}@bookTicker", pair.to_lowercase()))
        .collect::<Vec<String>>();
    let message = BinanceSubscriptionMessage {
        method: "SUBSCRIBE".to_owned(),
        params: channels,
    };
    dbg!(serde_json::to_string(&message).unwrap());
    serde_json::to_string(&message).expect("Failed to serialize subscription message")
}

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

fn get_default_public_url() -> String {
    "wss://data-stream.binance.vision/ws".to_owned()
}
fn get_default_private_url() -> String {
    "wss://data-stream.binance.vision/ws".to_owned()
}


async fn websocket_connection(url: &str, mut new_pairs: mpsc::Receiver<String>, mut messages: mpsc::Sender<BinancePairMessage>) {
    let pairs_registry: RwLock<HashSet<String>> = RwLock::new(HashSet::new());
    // This channel will be used to send messages to the websocket connection, for example when we want to subscribe/unsubscribe to a pair
    let (write_to_socket_sender, mut write_to_socket_receiver) = mpsc::channel::<Message>(32);
    // TODO stop the websocket when no pairs and start it only after 1 pairs
    select! {
        _ = async {
            while let Some(new_pair) = new_pairs.recv().await {
                if pairs_registry.read().await.contains(&new_pair) {
                    log::info!("Pair already exists in websocket: {}", new_pair);
                    continue;
                }
                log::info!("Adding pair to websocket: {}", new_pair);
                // @neil Do you know whether the write lock is released right after this line? Or do I need to wrap with a block?
                pairs_registry.write().await.insert(new_pair.clone());
                let mut temp_pairs = HashSet::new();
                temp_pairs.insert(new_pair.clone());
                write_to_socket_sender.send(Message::Text(create_binance_subscription_message(&temp_pairs).into())).await.expect("Failed to send subscription message");
            }
        } => {
            log::error!("Pair channel terminated");
        }
        _ = async {
            loop {
                // TODO waiting time should depend on the error we get from single websocket connection
                match single_websocket_connection(url, &pairs_registry, &mut messages, &mut write_to_socket_receiver, write_to_socket_sender.clone()).await {
                    Ok(_) => {}
                    Err(WebsocketConnectionError::ConnectionError(err)) => {
                        log::warn!("Failed to connection to websocket, retrying in 5 seconds {err}");
                        sleep(Duration::from_secs(5)).await;
                    }
                    Err(_) => {
                        log::error!("Dropping exchange");
                        break;
                    }
                }
            }
        } => {
            eprintln!("Websocket connection terminated");
        }
    }
    
}

async fn after_connection(pairs: &HashSet<String>, write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>) -> Result<(), WebsocketConnectionError> {
    let subscription_message = create_binance_subscription_message(&pairs);
    write.send(Message::Text(subscription_message.into())).await.map_err(|err| WebsocketConnectionError::SubscriptionError(err.to_string()))
}

// TODO this will be what we aim to generalize with hooks like "on pair added" and "exchange.after_connection"
async fn single_websocket_connection(url: &str, registry: &RwLock<HashSet<String>>, messages_to_exchange_sender: &mut mpsc::Sender<BinancePairMessage>,  write_to_socket_receiver: &mut mpsc::Receiver<Message>, write_to_socket_sender: mpsc::Sender<Message>) -> Result<(), WebsocketConnectionError> {
    let (ws_stream, _) =  tokio_tungstenite::connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();

    log::info!("Connected to websocket {}", url);
    let pairs = registry.read().await;
    // What we do after connecting will depend on the exchange
    after_connection(&pairs, &mut write).await?;
    
    let last_pong = RwLock::new(tokio::time::Instant::now());
    let mut pong_check_interval = time::interval(Duration::from_secs(5));
    // Read messages from the websocket and write to them when instructed. Also keep track of pings/pongs to detect disconnects
    select! {
        _ = async {
            loop {
                if let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        // Not the cleanest but binance doesn't always send a given tag we can use to differentiate. So using serde's tagged union is rather difficult
                        if text.contains("\"result\":") {
                            log::info!("Received result message: {}", text);
                            continue;
                        }
                        // Even here, when/if we ever decide to implement other channels, they all come with an "e" tag, but the bookTicker channel doesn't
                        match serde_json::from_str::<BinancePairMessage>(&text) {
                            Ok(pair_message) => {
                                if let Err(err) = messages_to_exchange_sender.send(pair_message).await {
                                    log::error!("Error sending message to channel: {err}");
                                    break;  // if the internal channel is closed, we should close the connection entirely TODO
                                }
                            }
                            Err(err) => {
                                log::warn!("Failed to parse message: {text}, {err}");
                            }
                        }
                    }
                    Ok(Message::Ping(ping)) => {
                        if let Err(err) = write_to_socket_sender.send(Message::Pong(ping)).await {
                            log::error!("Failed to send pong: {}", err);
                            break;
                        };
                    }
                    // This happens in response to pongs we send ourselves
                    Ok(Message::Pong(_)) => {
                        let now = tokio::time::Instant::now();
                        log::trace!("Received pong at {now:?}");
                        let mut last_pong = last_pong.write().await;
                        *last_pong = now;
                    }
                    Ok(other) => {
                        dbg!(other);
                        log::warn!("Received non-text message from websocket");
                    }
                    Err(err) => {
                        log::error!("Error reading message from websocket: {}", err);
                        break;
                    }
                }
            }
            }
        } => {
            log::error!("Websocket read terminated");
            return Err(WebsocketConnectionError::ConnectionError(tokio_tungstenite::tungstenite::Error::ConnectionClosed));
        }
        // Listen to messages that need to be forwarded to the websocket
        err = async {
            loop {
                if let Some(message) = write_to_socket_receiver.recv().await {
                    if let Err(err) = write.send(message).await {
                        log::error!("Failed to send message to websocket: {}", err);
                        break WebsocketConnectionError::WriteClosed(err.to_string());
                    }
                }
            }
        } => {
            log::error!("Write to socket terminated");
            return Err(err);
        }
        err = async {
            loop {
                pong_check_interval.tick().await;
                let last_pong = last_pong.read().await;
                if tokio::time::Instant::now() - *last_pong > Duration::from_secs(10) {
                    log::warn!("Pong check failed, closing connection. Last pong was {:?}", *last_pong);
                    break WebsocketConnectionError::PongError();
                }
                if let Err(err) = write_to_socket_sender.send(Message::Ping("".into())).await {
                    log::error!("Failed to send ping: {}", err);
                    break WebsocketConnectionError::WriteClosed(err.to_string());
                }
            }
        } => {
            log::error!("Pong check terminated");
            return Err(err);
        }
    }
}

