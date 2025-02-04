use thiserror::Error;

#[derive(Error, Debug)]
pub enum WebsocketConnectionError {
    #[error("Failed to connect to websocket: {0}")]
    ConnectionError(#[from]tokio_tungstenite::tungstenite::Error),
    #[error("Failed to read from websocket: {0}")]
    ReadClosed(String),
    #[error("Failed to write to websocket: {0}")]
    WriteClosed(String),
    #[error("Pong was not received from websocket")]
    PongError(),
    #[error("Failed to send channel subscription to websocket: {0}")]
    SubscriptionError(String),
    #[error("Channel was closed")]
    ChannelClosed(),
}

