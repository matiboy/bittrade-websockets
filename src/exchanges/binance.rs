use std::time::Duration;

use futures::{stream::SplitSink, SinkExt};
use serde::Deserialize;

use super::exchange::{HasPairs, PublicWebsocketApi, WebsocketApi};
use tokio::{net::TcpStream, sync::mpsc, time::sleep};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::exchanges::{errors::ExchangeApiError, messages::ExchangePairPrice};

// use super::exchange::{ExchangeName, ExchangeApi};


#[derive(Debug, Deserialize)]
struct BinancePairMessage {
    #[serde(rename = "s")]
    pair: String,
    #[serde(rename = "a")]
    ask: f64,
    #[serde(rename = "b")]
    bid: f64,
}

// impl TryFrom<Result<BinancePairMessage, serde_json::Error>> for ExchangePairMessage {
//     type Error = ();
//     fn try_from(value: Result<BinancePairMessage, serde_json::Error>) -> Result<Self, Self::Error> {
//         if let Ok(value) = value {
//             Ok(ExchangePairMessage {
//                 pair: value.pair,
//                 price: Price {
//                     ask: value.ask,
//                     bid: value.bid,
//                 }
//             })
//         } else {
//             Err(())
//         }
//     }
// }


// impl TryFrom<(ExchangeName, &str)> for ExchangePairMessage {
//     type Error = ();
//     fn try_from((exchange, message): (ExchangeName, &str)) -> Result<Self, Self::Error> {
//         match exchange {
//             ExchangeName::Binance => serde_json::from_str::<BinancePairMessage>(message).try_into(),
//             _ => unimplemented!(),
//         }
//     }
// }

pub fn create_binance_subscription_message(pairs: &Vec<String>) -> String {
    let channels = pairs.iter().map(|pair| format!(r#""{}@bookTicker""#, pair.to_lowercase().replace("_", ""))).collect::<Vec<String>>().join(", ");
    log::info!("Subscribing to channels: {}", channels);
    format!(r#"{{"method": "SUBSCRIBE", "params": [{}]}}"#, channels)
}

#[derive(Debug)]
pub struct BinanceExchange {
    pairs: Vec<String>,
}

impl BinanceExchange {
    pub fn new() -> Self {
        Self {
            pairs: Vec::new(),
        }
    }
}

impl HasPairs for BinanceExchange {
    async fn add_pair(&mut self, pair: &String) -> Result<(), ExchangeApiError> {
        self.pairs.push(pair.clone());
        Ok(())
    }
}

impl PublicWebsocketApi for BinanceExchange {
    async fn on_pair_added(&self, pair: String, write: &mpsc::Sender<Message>) -> Result<(), ExchangeApiError> {
        let subscription_message = create_binance_subscription_message(&vec![pair]);
        if let Err(err) = write.send(Message::from(subscription_message)).await {
            log::error!("Failed to send subscription message on internal channel: {}", err);
            return Err(ExchangeApiError::DropExchangeError("Failed to send subscription message on internal channel".to_owned()));
        }
        Ok(())
    }
}

impl WebsocketApi for BinanceExchange {
    async fn after_connection(&self, write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>) -> Result<(), ExchangeApiError> {
        // Send the subscription message
        let subscription_message = create_binance_subscription_message(&self.pairs);
        if let Err(err) = write.send(Message::from(subscription_message)).await {
            log::error!("Failed to send subscription message: {}", err);
            sleep(Duration::from_secs(5)).await;
            return Err(ExchangeApiError::DropConnectionError("Failed to send subscription message".to_owned()));
        }
        Ok(())
    }
    fn get_default_public_url(&self) -> &str {
        "wss://data-stream.binance.vision/ws"
    }
    fn get_default_private_url(&self) -> &str {
        "wss://data-stream.binance.vision/ws"
    }
}
