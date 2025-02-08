use std::{collections::{HashMap, HashSet}, time::Duration};

use futures::{stream::SplitSink, SinkExt, StreamExt};
use tokio::{net::TcpStream, select, sync::{broadcast, mpsc, oneshot, watch, RwLock}, task::JoinSet, time::{self, sleep}};
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::exchanges::{binance::messages::{create_binance_subscription_message, BinancePairMessage}, messages::ExchangePairPrice};
use crate::websocket::{WebsocketConnectionError, pong_check_interval_task};


async fn after_connection(pairs: &HashSet<String>, write: &mpsc::Sender<Message>) -> Result<(), WebsocketConnectionError> {
    let subscription_message = create_binance_subscription_message(&pairs);
    log::info!("On connection, sending subscription message: {}", subscription_message);
    write.send(Message::Text(subscription_message.into())).await.map_err(|err| WebsocketConnectionError::WriteClosed(err.to_string()))
}

// TODO this will be what we aim to generalize with hooks like "on pair added" and "exchange.after_connection"
async fn single_websocket_connection(url: &str, registry_receiver: watch::Receiver<HashMap<String, broadcast::Sender<ExchangePairPrice>>>) -> Result<(), WebsocketConnectionError> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();
    log::info!("Connected to websocket {url}");
    // We have a max of 1024 pairs on a single websocket
    let (write_to_websocket_sender, mut write_to_websocket_receiver) = mpsc::channel::<Message>(1024);
    let (tx, rx) = broadcast::channel::<BinancePairMessage>(1024);
    let listen_to_pairs = tokio::spawn(pairs_to_broadcast(registry_receiver, tx.clone(), write_to_websocket_sender.clone()));
        
    // What we do after connecting will depend on the exchange
    let last_pong = RwLock::new(tokio::time::Instant::now());
    let mut pong_check_interval = time::interval(Duration::from_secs(5));
    // Read messages from the websocket and write to them when instructed. Also keep track of pings/pongs to detect disconnects
    let outcome = select! {
        err = async {
            loop {
                if let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        log::debug!("{}", text);
                        // Not the cleanest but binance doesn't always send a given tag we can use to differentiate. So using serde's tagged union is rather difficult
                        if text.contains("\"result\":") {
                            log::info!("Received result message: {}", text);
                            continue;
                        }
                        // Even here, when/if we ever decide to implement other channels, they all come with an "e" tag, but the bookTicker channel doesn't
                        match serde_json::from_str::<BinancePairMessage>(&text) {
                            Ok(pair_message) => {
                                if let Err(err) = tx.send(pair_message) {
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
                        if let Err(err) = write_to_websocket_sender.send(Message::Pong(ping)).await {
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
            err
        }
        // Listen to messages that need to be forwarded to the websocket
        err = async {
            loop {
                if let Some(message) = write_to_websocket_receiver.recv().await {
                    if let Err(err) = write.send(message).await {
                        log::error!("Failed to send message to websocket: {}", err);
                        break WebsocketConnectionError::WriteClosed(err.to_string());
                    }
                }
            }
        } => {
            log::error!("Write to socket terminated");
            err
        }
        err = pong_check_interval_task(&mut pong_check_interval, &last_pong, &write_to_websocket_sender) => {
            log::error!("Pong check terminated");
            err
        }
    };
    listen_to_pairs.abort();
    Err(outcome)
}

async fn pairs_to_broadcast(mut registry_receiver: watch::Receiver<HashMap<String, broadcast::Sender<ExchangePairPrice>>>, tx: broadcast::Sender<BinancePairMessage>, write_to_websocket_sender: mpsc::Sender<Message>) {
    loop {
        // When the JoinSet is dropped in each loop, all tasks it has spawned will be aborted
        let mut set = JoinSet::new();
        if let Ok(_) = registry_receiver.changed().await {
            let pairs = registry_receiver.borrow().clone();
            for (pair, sender) in pairs.iter() {
                // NOTE: Binance doesn't mind that we register multiple times; if they ever do, we'll need to keep new/old value and do a diff
                log::info!("Subscribing to pair: {}", pair);
                let (mut rx, pair, sender) = (tx.subscribe(), pair.clone(), sender.clone());
                set.spawn(async move {
                    loop {
                        match rx.recv().await {
                            Ok(m) => {
                                if m.pair == *pair {
                                    sender.send(m.into());
                                }
                            },
                            Err(_) => break
                        }
                    }
                });
            }
            // Actually send to the websocket
            let _ = after_connection(&pairs.keys().cloned().collect(), &write_to_websocket_sender.clone()).await;
        }
    }
}

pub async fn reconnecting_websocket_connection(url: String, pairs_registry_channel_receiver: watch::Receiver<HashMap<String, broadcast::Sender<ExchangePairPrice>>>, interrupt: oneshot::Receiver<()>) -> Result<(), WebsocketConnectionError> {
    select! {
        e = async move {
            loop {
                let pairs_registry_channel_receiver = pairs_registry_channel_receiver.clone();
                match single_websocket_connection(&url, pairs_registry_channel_receiver).await {
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
        } => { e }
        _ = interrupt => {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::exchanges::exchange::ExchangeName;

    use super::*;
    use std::collections::HashSet;
    use tokio::sync::{mpsc, watch};
    use tokio::time::{sleep, Duration};
}
