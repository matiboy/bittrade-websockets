use std::collections::HashMap;

use tokio::{select, sync::{broadcast, mpsc, watch}};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;



pub async fn create_pair_listener<T, U, F>(mut rx: broadcast::Receiver<T>, sender: mpsc::Sender<U>, cancellation_token: CancellationToken, filter: F) -> () 
where 
    T: Into<U> + Clone + Send + 'static + std::fmt::Debug, 
    U: Send + 'static,
    F: Fn(&T) -> bool + Send + 'static
{
    select! {
        _ = cancellation_token.cancelled() => {
            log::info!("Received cancellation token, stopping listener");
        },
        _ = async {
            log::info!("Spawned task to listen");
            loop {
                match rx.recv().await {
                    Ok(m) => {
                        log::info!("Received message from channel, attempting to match {m:?}");
                        if filter(&m) {
                            let _ = sender.send(m.into()).await;
                        }
                    },
                    Err(_) => break
                }
            }
            log::error!("Channel closed, this should not happen unless we add more pairs");
        } => {}
    }
}
