use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub enum Exchange {
    Binance,
    Whitebit,
    Kraken,
    Bitfinex,
    Mexc,
    Coinbase,
}