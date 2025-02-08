use core::fmt;
use serde::{Deserialize, Serialize};


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