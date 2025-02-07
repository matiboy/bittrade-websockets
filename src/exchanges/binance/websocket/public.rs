use std::{collections::HashSet, time::Duration};

use futures::{stream::SplitSink, SinkExt, StreamExt};
use tokio::{net::TcpStream, select, sync::{mpsc, watch, RwLock}, time::{self, sleep}};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{control::control::ControlCommand, exchanges::binance::messages::{create_binance_subscription_message, BinancePairMessage}};
use crate::websocket::{WebsocketConnectionError, pong_check_interval_task};


pub async fn websocket_connection(url: &str, new_pairs: mpsc::Receiver<String>, messages: mpsc::Sender<BinancePairMessage>) {
    let pairs_registry: RwLock<HashSet<String>> = RwLock::new(HashSet::new());
    // This channel will be used to send messages to the websocket connection, for example when we want to subscribe/unsubscribe to a pair
    let (write_to_websocket_sender, mut write_to_websocket_receiver) = mpsc::channel::<Message>(32);
    let mut write_to_websocket_sender_clone = write_to_websocket_sender.clone();
    let (pairs_registry_channel_sender, mut pairs_registry_channel_receiver) = watch::channel::<HashSet<String>>(HashSet::new());
    // TODO stop the websocket when no pairs and start it only after 1 pairs
    select! {
        _ = listen_to_pairs_channel(new_pairs, &pairs_registry, &write_to_websocket_sender) => {
            log::error!("Pair channel terminated");
        }
        _ = reconnecting_websocket_connection(url, &pairs_registry, messages, &mut write_to_websocket_receiver, &mut write_to_websocket_sender_clone) => {
            log::error!("Websocket connection terminated");
        }
    }
}

async fn listen_to_pairs_channel_watch(mut new_pairs: mpsc::Receiver<ControlCommand>, watch_sender: &watch::Sender<HashSet<String>>) {
    let mut pairs = HashSet::<String>::new();
    loop {
        match new_pairs.recv().await {
            Some(ControlCommand::AddPair(_, pair)) => {
                pairs.insert(pair);
                let _ = watch_sender.send(pairs.clone()); // Send updated set
            }
            Some(ControlCommand::RemovePair(_, pair)) => {
                pairs.remove(&pair);
                let _ = watch_sender.send(pairs.clone()); // Send updated set
            }
            Some(_) => continue,
            None => break,
        }
    }
}

async fn listen_to_pairs_channel(mut new_pairs: mpsc::Receiver<String>, pairs_registry: &RwLock<HashSet<String>>, write_to_socket_sender: &mpsc::Sender<Message>) {
    while let Some(new_pair) = new_pairs.recv().await {
        let is_first_pair;
        {
            log::debug!("Got lock on pairs registry");
            let mut registry = pairs_registry.write().await;
            is_first_pair = registry.len() == 0;
            (*registry).insert(new_pair.clone());
            log::debug!("Released lock on pairs registry");
        }
        log::info!("[BINANCE] Adding pair to websocket: {new_pair}");
        // When we receive the first pair, we don't need to send a subscription message this will be done on connection
        if is_first_pair {
            continue;
        }
        let temp_pairs = HashSet::from([new_pair.clone()]);
        match write_to_socket_sender.send(Message::Text(create_binance_subscription_message(&temp_pairs).into())).await {
            Ok(_) => {
                log::info!("Sent subscription message for pair: {}", new_pair);
            }
            Err(err) => {
                log::error!("Failed to send subscription message: {}", err);
                break;
            }
        }
    }
}

async fn after_connection(pairs: &HashSet<String>, write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>) -> Result<(), WebsocketConnectionError> {
    let subscription_message = create_binance_subscription_message(&pairs);
    log::info!("On connection, sending subscription message: {}", subscription_message);
    write.send(Message::Text(subscription_message.into())).await.map_err(|err| WebsocketConnectionError::WriteClosed(err.to_string()))
}

// TODO this will be what we aim to generalize with hooks like "on pair added" and "exchange.after_connection"
async fn single_websocket_connection(url: &str, registry: &RwLock<HashSet<String>>, messages_to_exchange_sender: &mut mpsc::Sender<BinancePairMessage>,  write_to_socket_receiver: &mut mpsc::Receiver<Message>, write_to_socket_sender: mpsc::Sender<Message>) -> Result<(), WebsocketConnectionError> {
    let (ws_stream, _) =  tokio_tungstenite::connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();

    log::info!("Connected to websocket {url}");
    {
        let pairs = registry.read().await;
        after_connection(&pairs, &mut write).await?;
    }
    // What we do after connecting will depend on the exchange
    let last_pong = RwLock::new(tokio::time::Instant::now());
    let mut pong_check_interval = time::interval(Duration::from_secs(5));
    // Read messages from the websocket and write to them when instructed. Also keep track of pings/pongs to detect disconnects
    select! {
        err = async {
            loop {
                if let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        // Not the cleanest but binance doesn't always send a given tag we can use to differentiate. So using serde's tagged union is rather difficult
                        if text.contains("\"result\":") {
                            log::info!("Received result message: {}", text);
                            continue;
                        }
                        // Even here, when/if we ever decide to implement other channels, they all come with an "e" tag, but the bookTicker channel doesn't
                        match serde_json::from_str::<BinancePairMessage>(&text) {
                            Ok(pair_message) => {
                                if let Err(err) = messages_to_exchange_sender.send(pair_message).await {
                                    log::error!("Error sending message to channel: {err}");
                                    break WebsocketConnectionError::ChannelClosed("Messages to exchange channel".to_owned());
                                }
                            }
                            Err(err) => {
                                log::warn!("Failed to parse message: {text}, {err}");
                            }
                        }
                    }
                    Ok(Message::Ping(ping)) => {
                        if let Err(err) = write_to_socket_sender.send(Message::Pong(ping)).await {
                            log::error!("Failed to send pong: {}", err);
                            break WebsocketConnectionError::ChannelClosed("Write to socket sender".to_owned());
                        };
                    }
                    // This happens in response to pings we send ourselves
                    Ok(Message::Pong(_)) => {
                        let now = tokio::time::Instant::now();
                        log::trace!("Received pong at {now:?}");
                        let mut last_pong = last_pong.write().await;
                        *last_pong = now;
                    }
                    Ok(Message::Close(_)) => {
                        // We probably don't need to do anything as the socket will get closed anyway
                        log::warn!("Received close message from websocket");
                    }
                    Ok(other) => {
                        dbg!(other);
                        log::warn!("Received non-text message from websocket");
                    }
                    Err(err) => {
                        log::error!("Error reading message from websocket: {}", err);
                        break WebsocketConnectionError::ReadClosed(err.to_string());
                    }
                }
            }
            }
        } => {
            log::error!("Websocket read terminated");
            return Err(err);
        }
        // Listen to messages that need to be forwarded to the websocket
        err = async {
            loop {
                if let Some(message) = write_to_socket_receiver.recv().await {
                    if let Err(err) = write.send(message).await {
                        log::error!("Failed to send message to websocket: {}", err);
                        break WebsocketConnectionError::WriteClosed(err.to_string());
                    }
                }
            }
        } => {
            log::error!("Write to socket terminated");
            return Err(err);
        }
        err = pong_check_interval_task(&mut pong_check_interval, &last_pong, &write_to_socket_sender) => {
            log::error!("Pong check terminated");
            return Err(err);
        }
    }
}

async fn reconnecting_websocket_connection(url: &str, pairs_registry: &RwLock<HashSet<String>>, mut messages: mpsc::Sender<BinancePairMessage>, write_to_socket_receiver: &mut mpsc::Receiver<Message>, write_to_socket_sender: &mut mpsc::Sender<Message>) -> Result<(), WebsocketConnectionError> {
    loop {
        match single_websocket_connection(url, &pairs_registry, &mut messages, write_to_socket_receiver, write_to_socket_sender.clone()).await {
            Ok(_) => {}
            Err(WebsocketConnectionError::ChannelClosed(name)) => {
                log::error!("Channel closed, this is fatal. Dropping exchange");
                break Err(WebsocketConnectionError::ChannelClosed(name));
            }
            Err(WebsocketConnectionError::PongError()) => {
                log::warn!("Pong error, retrying immediately");
            }
            Err(err) => {
                log::warn!("Failed to connection to websocket, retrying in 5 seconds {err}");
                sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::control::control::ControlCommand;
    use crate::exchanges::exchange::ExchangeName;

    use super::*;
    use std::collections::HashSet;
    use tokio::sync::{mpsc, watch};
    use tokio::time::{sleep, Duration};

    
    #[tokio::test]
    async fn test_adds_values_to_hashset() {
        let (tx, rx) = mpsc::channel::<ControlCommand>(10);
        let (watch_tx, watch_rx) = watch::channel(HashSet::new());

        // Spawn function
        let watch_tx_clone = watch_tx.clone();
        let handle = tokio::spawn(async move {
            listen_to_pairs_channel_watch(rx, &watch_tx_clone).await;
        });

        // Send messages
        tx.send(ControlCommand::AddPair(ExchangeName::Binance, "ETH/USD".to_owned())).await.unwrap();
        tx.send(ControlCommand::AddPair(ExchangeName::Binance, "XRP/USD".to_owned())).await.unwrap();
        tx.send(ControlCommand::AddPair(ExchangeName::Binance, "ETH/USD".to_owned())).await.unwrap(); // Duplicate should not be added

        sleep(Duration::from_millis(100)).await; // Allow async tasks to complete

        // Read latest value from watch receiver
        let latest_set = watch_rx.borrow().clone();

        assert_eq!(latest_set.len(), 2); // Should only contain BTC/USD and ETH/USD
        assert!(latest_set.contains("XRP/USD"));
        assert!(latest_set.contains("ETH/USD"));

        handle.abort(); // Clean up task
    }
    
    #[tokio::test]
    async fn test_removes_values_from_hashset() {
        let (tx, rx) = mpsc::channel::<ControlCommand>(10);
        let (watch_tx, watch_rx) = watch::channel(HashSet::new());

        // Spawn function
        let watch_tx_clone = watch_tx.clone();
        let handle = tokio::spawn(async move {
            listen_to_pairs_channel_watch(rx, &watch_tx_clone).await;
        });

        // Send messages
        tx.send(ControlCommand::AddPair(ExchangeName::Binance, "XRP/USD".to_owned())).await.unwrap();
        tx.send(ControlCommand::AddPair(ExchangeName::Binance, "ETH/USD".to_owned())).await.unwrap();
        tx.send(ControlCommand::RemovePair(ExchangeName::Binance, "ETH/USD".to_owned())).await.unwrap();
        tx.send(ControlCommand::RemovePair(ExchangeName::Binance, "ETH/USD".to_owned())).await.unwrap(); // No errors even if value isn't found

        sleep(Duration::from_millis(100)).await; // Allow async tasks to complete

        // Read latest value from watch receiver
        let latest_set = watch_rx.borrow().clone();

        assert_eq!(latest_set.len(), 1); // Should only contain BTC/USD and ETH/USD
        assert!(latest_set.contains("XRP/USD"));

        handle.abort(); // Clean up task
    }
    
    #[tokio::test]
    async fn test_ignores_other_commands() {
        let (tx, rx) = mpsc::channel::<ControlCommand>(10);
        let (watch_tx, watch_rx) = watch::channel(HashSet::new());

        // Spawn function
        let watch_tx_clone = watch_tx.clone();
        let handle = tokio::spawn(async move {
            listen_to_pairs_channel_watch(rx, &watch_tx_clone).await;
        });

        // Send messages
        tx.send(ControlCommand::AddPair(ExchangeName::Binance, "ETH/USD".to_owned())).await.unwrap();
        tx.send(ControlCommand::AddKey("dsa".to_owned(), "ETH/USD".to_owned())).await.unwrap();

        sleep(Duration::from_millis(100)).await; // Allow async tasks to complete

        // Read latest value from watch receiver
        let latest_set = watch_rx.borrow().clone();

        assert_eq!(latest_set.len(), 1); // Should only contain BTC/USD and ETH/USD
        assert!(latest_set.contains("ETH/USD"));

        handle.abort(); // Clean up task
    }
    
    #[tokio::test]
    async fn test_terminates_when_channel_is_closed() {
        let (tx, rx) = mpsc::channel::<ControlCommand>(10);
        let (watch_tx, watch_rx) = watch::channel(HashSet::new());

        // Spawn function
        let watch_tx_clone = watch_tx.clone();
        let handle = tokio::spawn(async move {
            listen_to_pairs_channel_watch(rx, &watch_tx_clone).await;
        });

        drop(tx); // Close channel
        sleep(Duration::from_millis(10)).await; // Allow async tasks to complete

        // Read latest value from watch receiver
        let latest_set = watch_rx.borrow().clone();

        assert_eq!(latest_set.len(), 0); // Should only contain BTC/USD and ETH/USD

        assert_eq!(handle.await.unwrap(), ());
    }
}
