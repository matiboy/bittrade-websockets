use tokio::{select, sync::{broadcast, oneshot}, task::JoinHandle};

use crate::exchanges::messages::ExchangePairPrice;


#[derive(Debug)]
pub struct ExchangeHandler {
    stop_sender: oneshot::Sender<()>,
    task: JoinHandle<()>,
    sender: broadcast::Sender<ExchangePairPrice>,
}

impl ExchangeHandler {
    pub async fn bridge(sender: broadcast::Sender<ExchangePairPrice>) -> Self {
        let (stop_sender, stop_receiver) = oneshot::channel();
        Self {
            stop_sender,
            sender,
            task: tokio::spawn(async move {
                loop {
                    select! {
                        _ = stop_receiver => {
                            break;
                        }
                    }
                }
            })
        }
    }
}