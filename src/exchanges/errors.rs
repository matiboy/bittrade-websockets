use thiserror::Error;


#[derive(Error, Debug)]
pub enum ExchangeApiError {
    #[error("Drop connection error: {0}")]
    DropConnectionError(String),
    #[error("Drop exchange error: {0}")]
    DropExchangeError(String),
}
