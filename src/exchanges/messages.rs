use super::exchange::ExchangeName;


#[derive(Debug, Clone)]
pub struct ExchangePairPrice {
    pub exchange: ExchangeName,
    pub pair: String,
    pub ask: f64,
    pub bid: f64,
}
