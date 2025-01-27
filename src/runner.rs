use std::{collections::{HashMap, HashSet}, env};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio::{net::{UnixListener, UnixStream}, select, sync::mpsc, time::{sleep, Duration}};
use futures::{ SinkExt, StreamExt };
use tokio_tungstenite::tungstenite::Message;

use crate::exchanges::Exchange;

async fn websocket_connection(exchange: Exchange, url: &str, pairs: Vec<String>, messages: mpsc::Sender<ExchangePairMessage>) {
    loop {
        // Connect to the websocket
        let ws_stream = match tokio_tungstenite::connect_async(url).await {
            Ok((stream, _)) => stream,
            Err(err) => {
                eprintln!("Failed to connect to websocket: {}. Retrying in 5 seconds...", err);
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        let (mut write, mut read) = ws_stream.split();

        // What we do after connecting will depend on the exchange
        match exchange {
            Exchange::Binance => {
                // Send the subscription message
                let subscription_message = create_binance_subscription_message(&pairs);
                if let Err(err) = write.send(Message::from(subscription_message)).await {
                    eprintln!("Failed to send subscription message: {}", err);
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }
            _ => {}
        }

        // Read messages from the websocket
        while let Some(message) = read.next().await {
            match message {
            Ok(Message::Text(text)) => {
                if let Ok(pair_message) = ExchangePairMessage::try_from((exchange.clone(), text.as_str())) {
                    if let Err(err) = messages.send(pair_message).await {
                        eprintln!("Error sending message to channel: {}", err);
                        break;
                    }
                } else {
                    eprintln!("Failed to parse message: {}", text);
                }
            }
            Ok(_) => {}
            Err(err) => {
                eprintln!("Error reading message from websocket: {}", err);
                break;
            }
            }
        }
    }
}

struct ExchangeManager {
    exchange: Exchange,
    socket: UnixListener,
    pairs: HashSet<String>,
}

impl ExchangeManager {
    async fn new(exchange: Exchange) -> Self {
        // Shoud depend on the exchange
        let websocket_url = env::var("PUBLIC_WEBSOCKET").unwrap_or_else(|_| "wss://data-stream.binance.vision".to_string());
        Self {
            exchange,
            socket: UnixListener::bind("/tmp/binance_manager.sock").unwrap(),
            pairs: HashSet::new(),
        }
    }

    async fn run(&mut self) {
        let (messages_sender, mut messages_receiver) = mpsc::channel::<ExchangePairMessage>(32);
        let pairs = self.pairs.iter().cloned().collect::<Vec<String>>();
        let pairs = vec!("btcusdt".to_string());
        let exchange = self.exchange.clone();
        let url = env::var("PUBLIC_WEBSOCKET").unwrap_or_else(|_| "wss://data-stream.binance.vision".to_string());
        loop {
            select! {
                Ok((stream, _)) = self.socket.accept() => {
                    while let Some(message) = messages_receiver.recv().await {
                        if let Ok(message) = serde_json::to_string(&message) {
                            if let Err(err) = stream.try_write(message.as_bytes()) {
                                eprintln!("Failed to write to socket: {}", err);
                                break;
                            }
                        } else {
                            eprintln!("Failed to serialize message");
                        }
                    }
                }
                // TODO we don't need exchange anymore here. Pairs however should be a channel
                _ = websocket_connection(exchange.clone(), &url, pairs.clone(), messages_sender.clone()) => {
                    eprintln!("Websocket connection terminated");
                }
                message = messages_receiver.recv() => {
                    if let Some(message) = message {
                        println!("Received message: {:?}", message);
                        
                    } else {
                        eprintln!("Failed to receive message");
                        continue;
                    }
                },
            }
        }
    }
}

pub async fn run_public_websocket(mut pairs_channel: mpsc::Receiver<String>) -> () {
    let mut binance_manager = ExchangeManager::new(Exchange::Binance).await;
    // For now we only support a single websocket connection
    let (messages_sender, mut messages_receiver) = mpsc::channel::<ExchangePairMessage>(32);

    loop {
        select! {
            _ = binance_manager.run() => {
                eprintln!("Binance manager terminated");
            },
        }
    }
}

fn create_binance_subscription_message(pairs: &Vec<String>) -> String {
    let channels = pairs.iter().map(|pair| format!(r#""{}@bookTicker""#, pair)).collect::<Vec<String>>().join(", ");
    format!(r#"{{"method": "SUBSCRIBE", "params": [{}]}}"#, channels)
}

#[derive(Debug, Serialize)]
pub struct PairPrice {
    pair: String,
    ask: Decimal,
    bid: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ExchangePairMessage {
    exchange: Exchange,
    price: PairPrice,
}

#[derive(Debug, Deserialize)]
struct BinancePairMessage {
    #[serde(rename = "s")]
    pair: String,
    #[serde(rename = "a")]
    ask: Decimal,
    #[serde(rename = "b")]
    bid: Decimal,
}

impl TryFrom<Result<BinancePairMessage, serde_json::Error>> for PairPrice {
    type Error = ();
    fn try_from(value: Result<BinancePairMessage, serde_json::Error>) -> Result<Self, Self::Error> {
        if let Ok(value) = value {
            Ok(PairPrice {
                pair: value.pair,
                ask: value.ask,
                bid: value.bid,
            })
        } else {
            Err(())
        }
    }
}


impl TryFrom<(Exchange, &str)> for ExchangePairMessage {
    type Error = ();
    fn try_from((exchange, message): (Exchange, &str)) -> Result<Self, Self::Error> {
        let price = match exchange {
            Exchange::Binance => serde_json::from_str::<BinancePairMessage>(message).try_into(),
            _ => unimplemented!(),
        };
        if let Ok(price) = price {
            Ok(ExchangePairMessage {
                exchange,
                price,
            })
        } else {
            Err(())
        }
    }
}

