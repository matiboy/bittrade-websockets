use super::exchange::ExchangeName;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ExchangePairPrice {
    #[serde(skip)]
    pub exchange: ExchangeName,
    #[serde(skip)]
    pub pair: String,
    #[serde(rename = "a")]
    pub ask: f64,
    #[serde(rename = "b")]
    pub bid: f64,
}