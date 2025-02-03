use std::{collections::HashMap, env, sync::Arc};

use rust_decimal::Decimal;
use serde::Serialize;
use tokio::{net::UnixListener, select, sync::{broadcast, mpsc, oneshot, RwLock}, task::JoinHandle, time::{self, sleep, Duration}};
use futures::{ SinkExt, StreamExt };
use tokio_tungstenite::tungstenite::Message;
use thiserror::Error;
// use tracing;


use crate::exchanges::{exchange::ExchangeName, manager::ExchangeManager};


pub async fn run(pairs_receiver: mpsc::Receiver<(ExchangeName, String)>) {
    let mut manager = ExchangeManager::new();
    log::info!("Manager created");
    // TODO Any reason why we're using a select here? What else do we expect to be running?
    select! {
        _ = manager.run(pairs_receiver) => {
            log::error!("Manager terminated");
        }
    }
}