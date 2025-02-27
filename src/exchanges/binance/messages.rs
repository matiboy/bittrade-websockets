use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use crate::{exchanges::{exchange::ExchangeName, messages::ExchangePairPrice}, json::deserialize_f64_from_string};


#[derive(Debug, Deserialize, Clone)]
pub struct BinancePairMessage {
    #[serde(rename = "s")]
    pub pair: String,
    #[serde(rename = "a", deserialize_with = "deserialize_f64_from_string")]
    pub ask: f64,
    #[serde(rename = "b", deserialize_with = "deserialize_f64_from_string")]
    pub bid: f64,
}

impl From<BinancePairMessage> for ExchangePairPrice {
    fn from(message: BinancePairMessage) -> Self {
        ExchangePairPrice {
            exchange: ExchangeName::Binance,
            pair: message.pair,
            ask: message.ask,
            bid: message.bid,
        }
    }
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
    serde_json::to_string(&message).expect("Failed to serialize subscription message")
}
