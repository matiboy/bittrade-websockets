use std::{collections::HashMap, env, sync::Arc};

use rust_decimal::Decimal;
use serde::Serialize;
use tokio::{net::UnixListener, select, sync::{broadcast, mpsc, oneshot, RwLock}, task::JoinHandle, time::{self, sleep, Duration}};
use futures::{ SinkExt, StreamExt };
use tokio_tungstenite::tungstenite::Message;
use thiserror::Error;
// use tracing;


use crate::exchanges::{binance::{}, exchange::{ExchangeName, WebsocketApi}, manager::ExchangeManager};

// type PairPriceSender = (String, mpsc::Sender<Price>);

// async fn websocket_connection<'a, T>(exchange: &'a T, url: &str, mut new_pairs: mpsc::Receiver<PairPriceSender>) 
// where   T: WebsocketApi + 'a + ?Sized,
//         ExchangeName: From<&'a T>
// {
//     let registry: RwLock<HashMap<String, mpsc::Sender<Price>>> = RwLock::new(HashMap::new());
//     let (write_to_socket_sender, mut write_to_socket_receiver) = mpsc::channel::<Message>(128);
//     select! {
//         _ = async {
//             while let Some((pair, sender)) = new_pairs.recv().await {
//                 registry.write().await.insert(pair.clone(), sender);
//                 if let Err(_) = exchange.on_pair_added(&pair, &write_to_socket_sender).await { break;}; // Here we only send via the channel, so any error isn't related to the websocket connection - it's the channel itself and that needs to be interrupted entirely
//             }
//         } => {
//             log::error!("Pair channel terminated");
//         }
//         _ = async {
//             'full: loop {
//                 // A strange idea, allows us to "continue" early but still wait before reconnect at the end. This should be replaced with a simple function
//                 'connection_attempt: for _ in 0..1 {
//                     // Connect to the websocket
//                     let ws_stream = match tokio_tungstenite::connect_async(url).await {
//                         Ok((stream, _)) => stream,
//                         Err(err) => {
//                             log::error!("Failed to connect to websocket {}: {}. Retrying in 5 seconds...", url, err);
//                             break 'connection_attempt;
//                         }
//                     };
//                     let (mut write, mut read) = ws_stream.split();
            
//                     log::info!("Connected to websocket {}", url);
//                     let pairs = registry.read().await;
//                     let pairs = pairs.keys().collect::<Vec<&String>>();
//                     // What we do after connecting will depend on the exchange
//                     match exchange.after_connection(&pairs, &mut write).await {
//                         Ok(_) => {}
//                         Err(ExchangeApiError::DropConnectionError(e)) => {
//                             log::error!("Failed to process after connection: {}. Reconnecting", e);
//                             break 'connection_attempt;
//                         }
//                         Err(ExchangeApiError::DropExchangeError(e)) => {
//                             log::error!("Failed to process after connection: {}. Exiting exchange", e);
//                             break 'full;
//                         }
//                     }
                    
//                     let last_pong = RwLock::new(tokio::time::Instant::now());
//                     let mut pong_check_interval = time::interval(Duration::from_secs(5));
//                     // Read messages from the websocket and write to them when instructed. Also keep track of pings/pongs to detect disconnects
//                     'single_connection: loop {
//                         select! {
//                             _ = async {
//                                 while let Some(message) = read.next().await {
//                                     match message {
//                                         Ok(Message::Text(text)) => {
//                                             if let Ok(pair_message) = ExchangePairMessage::try_from((ExchangeName::from(&exchange), text.as_str())) {
//                                                 if let Some(channel) = registry.read().await.get(&pair_message.pair) {
//                                                     if let Err(err) = channel.send(pair_message.price).await {
//                                                         log::error!("Error sending message to channel: {}", err);
//                                                         break;  // if the internal channel is closed, we should close the connection entirely TODO
//                                                     }
//                                                 } else {
//                                                     log::error!("Received message for unregistered pair: {}", pair_message.pair);
//                                                 }
//                                             } else {
//                                                 log::warn!("Failed to parse message: {}", text);
//                                             }
//                                         }
//                                         Ok(Message::Ping(ping)) => {
//                                             if let Err(err) = write_to_socket_sender.send(Message::Pong(ping)).await {
//                                                 log::error!("Failed to send pong: {}", err);
//                                                 break;
//                                             };
//                                         }
//                                         Ok(Message::Pong(_)) => {
//                                             log::trace!("Received pong");
//                                             let mut last_pong = last_pong.write().await;
//                                             *last_pong = tokio::time::Instant::now();
//                                         }
//                                         Ok(other) => {
//                                             dbg!(other);
//                                             log::warn!("Received non-text message from websocket");
//                                         }
//                                         Err(err) => {
//                                             log::error!("Error reading message from websocket: {}", err);
//                                             break;
//                                         }
//                                     }
//                                 }
//                             } => {
//                                 log::error!("Websocket read terminated");
//                                 break 'single_connection;
//                             }
//                             _ = async {
//                                 while let Some(message) = write_to_socket_receiver.recv().await {
//                                     if let Err(err) = write.send(message).await {
//                                         log::error!("Failed to send message to websocket: {}", err);
//                                         break;
//                                     }
//                                 }
//                             } => {
//                                 log::error!("Write to socket terminated");
//                                 break 'single_connection;
//                             }
//                             _ = async {
//                                 loop {
//                                     pong_check_interval.tick().await;
//                                     let last_pong = last_pong.read().await;
//                                     if tokio::time::Instant::now() - *last_pong > Duration::from_secs(10) {
//                                         log::warn!("Pong check failed, closing connection. Last pong was {:?}", *last_pong);
//                                         break;
//                                     }
//                                     if let Err(err) = write_to_socket_sender.send(Message::Ping("".into())).await {
//                                         log::error!("Failed to send ping: {}", err);
//                                         break;  // TODO this is internal channel failure, this should close this entire manager
//                                     }
//                                 }
//                             } => {
//                                 log::error!("Pong check terminated");
//                                 break 'single_connection;
//                             }
//                         }
//                     }
//                 }
//                 sleep(Duration::from_secs(7)).await;
//             }
//         } => {
//             eprintln!("Websocket connection terminated");
//         }
//     }
    
// }

// #[derive(Debug)]
// struct PairManager {
//     task_handle: Option<JoinHandle<()>>,
// }

// #[derive(Error, Debug)]
// pub enum PairError {
//     #[error("Channel send error: {0}")]
//     ChannelSendError(String),

//     #[error("Other error: {0}")]
//     ParsingError(String),
// }

// impl From<serde_json::Error> for PairError {
//     fn from(err: serde_json::Error) -> Self {
//         PairError::ParsingError(err.to_string())
//     }
// }
// impl From<broadcast::error::SendError<String>> for PairError {
//     fn from(err: broadcast::error::SendError<String>) -> Self {
//         PairError::ChannelSendError(err.to_string())
//     }
// }

// impl PairManager {
//     fn new(exchange_name: String, pair: String, mut from_exchange_messages: mpsc::Receiver<Price>) -> Self {
//         let socket_path = format!("/tmp/{}_{}.sock", exchange_name, pair.replace("/", "_"));
//         log::info!("Creating socket at {}", socket_path);
//         if std::path::Path::new(&socket_path).exists() {
//             std::fs::remove_file(&socket_path).expect("Failed to remove existing socket file");
//         }
//         let mut socket = UnixListener::bind(socket_path).expect("Failed to bind socket");
//         let th = tokio::spawn(async move {
//             let (to_sockets_messages_sender, mut to_sockets_messages_receiver) = broadcast::channel::<String>(1024);
//             let current_value = RwLock::new(None);
//             select! {
//                 // Doing the below branch just to keep the receiver from being dropped, which would close the channel. It also allows us to keep the current value in the RwLock which can be used for new unix socket connections
//                 _ = async {
//                     while let Ok(message) = to_sockets_messages_receiver.recv().await {
//                         log::trace!("Received message inside broadcast channel: {:?}", message);
//                         let mut current_value = current_value.write().await;
//                         *current_value = Some(message);
//                     }
//                 } => {
//                     log::error!("Broadcast channel terminated");
//                 }
//                 _ = accept_connections(&mut socket, to_sockets_messages_sender.clone(), &current_value) => {
//                     log::error!("Accept connections terminated");
//                 }
//                 _ = async {
//                     let mut latest_price: Option<Price> = None;
//                     while let Some(message) = from_exchange_messages.recv().await {
//                         log::debug!("Received message inside pair manager: {:?}", message);
//                         // Avoid sending the same message twice
//                         if let Some(ref latest_price) = latest_price {
//                             if latest_price == &message {
//                                 continue;
//                             }
//                         }
//                         latest_price = Some(message.clone());

//                         match {
//                             let message: String = serde_json::to_string(&message)?;
//                             to_sockets_messages_sender.send(message)?;
//                             Ok::<(), PairError>(())
//                         } {
//                             Err(PairError::ChannelSendError(err)) => {
//                                 log::error!("Failed to send message to socket: {}", err);
//                                 break;
//                             }
//                             Err(PairError::ParsingError(err)) => {
//                                 log::warn!("Failed to parse message: {}", err);
//                             }
//                             _ => {
//                                 log::debug!("Message sent to socket");
//                             }
//                         };
//                     }
//                     Ok::<(), PairError>(())
//                 } => {
//                     log::error!("Connection channel terminated");
//                 }
//             }
//         });
//         Self {
//             task_handle: th.into(),
//         }
//     }
// }



// pub async fn run_public_websocket(mut channel: mpsc::Receiver<(Exchange, String)>) -> () {
//     let mut managers: HashMap<Exchange, ExchangeManager> = HashMap::new();
    
//     while let Some((exchange, pair)) = channel.recv().await {
//         log::info!("Public socket received pair: {:?} - {}", exchange, pair);
//         if let Some(manager) = managers.get_mut(&exchange) {
//             log::info!("Adding pair to existing manager {:?}", exchange);
//             manager.add_pair(pair).await;
//         } else {
//             log::info!("Manager not found for exchange {:?}, creating", exchange);
//             let mut new_manager = ExchangeManager::new(exchange.clone()).await;
//             new_manager.add_pair(pair).await;
//             managers.insert(exchange.clone(), new_manager);
//         };
//     }
// }


pub async fn run(pairs_receiver: mpsc::Receiver<(ExchangeName, String)>) {
    let mut manager = ExchangeManager::new();
    log::info!("Manager created");
    select! {
        _ = manager.run(pairs_receiver) => {
            log::error!("Manager terminated");
        }
    }
}