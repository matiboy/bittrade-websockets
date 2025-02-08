use std::collections::{HashMap, HashSet};

use tokio::{select, sync::{broadcast, mpsc, oneshot, watch}};

use crate::{exchanges::{binance::websocket::public::reconnecting_websocket_connection, exchange::ExchangeName, messages::ExchangePairPrice}};

use super::{commands::Command, messages::BinancePairMessage};

#[derive(Debug)]
pub struct BinanceExchange {
    public_websocket_handle: Option<tokio::task::JoinHandle<()>>,  // TODO we should not need to keep the handle anymore, the oneshot stopper should suffice
    public_websocket_interrupt: Option<oneshot::Sender<()>>,
    pairs_sender: watch::Sender<HashMap<String, broadcast::Sender<ExchangePairPrice>>>,
    pairs_receiver: watch::Receiver<HashMap<String, broadcast::Sender<ExchangePairPrice>>>,
}

impl BinanceExchange {
    pub fn new() -> Self {
        let (pairs_sender, pairs_receiver) = watch::channel(HashMap::new());
        Self {
            public_websocket_handle: None,
            pairs_sender,
            pairs_receiver,
            public_websocket_interrupt: None,
        }
    }

    fn get_name(&self) -> String {
        (&ExchangeName::Binance).into()
    }

    fn get_exchange_matching_pair(&self, pair: &str) -> String {
        pair.replace("_", "")
    }

    pub async fn add_pair(&mut self, pair: &String, sender: broadcast::Sender<ExchangePairPrice>) {
        let exchange_pair = self.get_exchange_matching_pair(pair);
        let mut current_pairs = self.pairs_receiver.borrow().clone();
        if current_pairs.contains_key(&exchange_pair) {
            log::info!("[{}] Pair {} already exists", self.get_name(), pair);
            return;
        }
        // Add the pair to the list
        current_pairs.insert(pair.clone(), sender);
        // TODO: We should detect this and what, recreate the exchange?
        self.pairs_sender.send(current_pairs).expect("Failed to send new pairs");
        if self.public_websocket_interrupt.is_none() {
            let pairs_receiver = self.pairs_receiver.clone();
            let public_url = 
        std::env::var("BINANCE_PUBLIC_WS_URL").unwrap_or_else(|_| get_default_public_url());
            let (interrupt_tx, interrupt_rx) = oneshot::channel();
            self.public_websocket_interrupt = Some(interrupt_tx);
            tokio::spawn(reconnecting_websocket_connection(public_url.clone(), pairs_receiver, interrupt_rx));
        }
    }

    // pub async fn remove_pair(&mut self, pair: &String) -> () {
    //     let pair = self.get_exchange_matching_pair(pair);
    //     let mut pairs = self.pairs_receiver.borrow().clone();
    //     if !pairs.contains(&pair) {
    //         log::info!("Pair {} does not exist", pair);
    //         return;
    //     }
    //     // Remove the pair from the list
    //     pairs.remove(&pair);
    //     if pairs.is_empty() {
    //         self.stop_public();
    //         return;
    //     }
    //     // TODO If this fails this should terminate the exchange and force it to restart
    //     self.pairs_sender.send(pairs).expect("Failed to send new pairs");
    // }

    fn stop_public(&mut self) {
        // NOTE: .take puts None in place
        if let Some(interrupt) = self.public_websocket_interrupt.take() {
            let _ = interrupt.send(());
        }
    }

}

impl Drop for BinanceExchange {
    fn drop(&mut self) {
        log::info!("Dropping Binance exchange");
        self.stop_public();
    }
}


pub fn get_default_public_url() -> String {
    "wss://data-stream.binance.vision/ws".to_owned()
}
pub fn get_default_private_url() -> String {
    "wss://data-stream.binance.vision/ws".to_owned()
}
