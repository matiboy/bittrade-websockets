use tokio::{select, sync::mpsc};

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