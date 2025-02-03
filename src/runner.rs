use std::{collections::HashMap, env, sync::Arc};

use rust_decimal::Decimal;
use serde::Serialize;
use tokio::{net::UnixListener, select, sync::{broadcast, mpsc, oneshot, RwLock}, task::JoinHandle, time::{self, sleep, Duration}};
use futures::{ SinkExt, StreamExt };
use tokio_tungstenite::tungstenite::Message;
use thiserror::Error;
// use tracing;


use crate::exchanges::{exchange::ExchangeName, manager::ExchangeManager};


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