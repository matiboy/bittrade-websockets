use core::fmt;
use std::{collections::HashMap, env, str::FromStr};

use futures::stream::SplitSink;
use serde::{Deserialize, Serialize};
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use thiserror::Error;

use super::errors::ExchangeApiError;


pub const BINANCE: &str = "binance";
pub const WHITEBIT: &str = "whitebit";
pub const KRAKEN: &str = "kraken";
pub const BITFINEX: &str = "bitfinex";
pub const MEXC: &str = "mexc";
pub const COINBASE: &str = "coinbase";
pub const INDEPENDENT_RESERVE: &str = "independent_reserve";

#[derive(Debug, Clone, Hash, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExchangeName {
    Binance,
    // Whitebit,
    // Kraken,
    // Bitfinex,
    // Mexc,
    // Coinbase,
    // IndepedentReserve,
}

impl From<&ExchangeName> for String {
    fn from(value: &ExchangeName) -> Self {
        match value {
            ExchangeName::Binance => BINANCE.to_owned(),
            // Exchange::Whitebit => WHITEBIT.to_owned(),
            // Exchange::Kraken => KRAKEN.to_owned(),
            // Exchange::Bitfinex => BITFINEX.to_owned(),
            // Exchange::Mexc => MEXC.to_owned(),
            // Exchange::Coinbase => COINBASE.to_owned(),
            // Exchange::IndepedentReserve => INDEPENDENT_RESERVE.to_owned(),
        }
    }
}

impl From<ExchangeName> for &str {
    fn from(value: ExchangeName) -> Self {
        match value {
            ExchangeName::Binance => BINANCE,
            // Exchange::Whitebit => WHITEBIT,
            // Exchange::Kraken => KRAKEN,
            // Exchange::Bitfinex => BITFINEX,
            // Exchange::Mexc => MEXC,
            // Exchange::Coinbase => COINBASE,
            // Exchange::IndepedentReserve => INDEPENDENT_RESERVE,
        }
    }
}

impl From<&str> for ExchangeName {
    fn from(value: &str) -> Self {
        match value {
            BINANCE => ExchangeName::Binance,
            // WHITEBIT => Exchange::Whitebit,
            // KRAKEN => Exchange::Kraken,
            // BITFINEX => Exchange::Bitfinex,
            // MEXC => Exchange::Mexc,
            // COINBASE => Exchange::Coinbase,
            // INDEPENDENT_RESERVE => Exchange::IndepedentReserve,
            _ => unimplemented!(),
        }
    }
}

impl fmt::Display for ExchangeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExchangeName::Binance => write!(f, "{BINANCE}"),
            // Exchange::Whitebit => write!(f, "{}", WHITEBIT),
            // Exchange::Kraken => write!(f, "{}", KRAKEN),
            // Exchange::Bitfinex => write!(f, "{}", BITFINEX),
            // Exchange::Mexc => write!(f, "{}", MEXC),
            // Exchange::Coinbase => write!(f, "{}", COINBASE),
            // Exchange::IndepedentReserve => write!(f, "{}", INDEPENDENT_RESERVE),
        }
    }
}

// pub trait WebsocketApi {
//     async fn after_connection(&self, write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>) -> Result<(), ExchangeApiError>;
//     fn get_default_public_url(&self) -> &str;
//     fn get_default_private_url(&self) -> &str;
//     async fn get_public_url(&self) -> String {
//         env::var("PUBLIC_WEBSOCKET").unwrap_or_else(|_| self.get_default_public_url().to_string())
//     }
//     async fn get_private_url(&self) -> String {
//         env::var("PRIVATE_WEBSOCKET").unwrap_or_else(|_| self.get_default_private_url().to_string())
//     }
// }

// #[derive(Debug)]
// pub struct GenericExchange<T> 
// where T: HasPairs
// {
//     specific_exchange: T,
//     pairs: HashMap<String, i8>,
// }

// impl<T: HasPairs> GenericExchange<T> {
//     pub fn new(specific_exchange: T) -> Self {
//         Self {
//             specific_exchange,
//             pairs: HashMap::new(),
//         }
//     }
//     pub async fn add_pair(&mut self, pair: &String) {
//         if let Some(count) = self.pairs.get_mut(pair) {
//             *count += 1;
//         } else {
//             self.pairs.insert(pair.clone(), 1);
//             self.specific_exchange.add_pair(pair).await;
//         }
//     }
// }
