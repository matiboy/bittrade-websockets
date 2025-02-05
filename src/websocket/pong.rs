use std::time::Duration;

use tokio::{sync::{mpsc, RwLock}, time};
use tokio_tungstenite::tungstenite::Message;

use super::errors::WebsocketConnectionError;


pub async fn pong_check_interval_task(pong_check_interval: &mut time::Interval, last_pong: &RwLock<tokio::time::Instant>, write_to_socket_sender: &mpsc::Sender<Message>) -> WebsocketConnectionError {
    // Note: we could have pong_check_interval internal to this function but that would make it harder to test
    loop {
        pong_check_interval.tick().await;
        let last_pong = last_pong.read().await;
        if tokio::time::Instant::now() - *last_pong > Duration::from_secs(10) {
            log::warn!("Pong check failed, closing connection. Last pong was {:?}", *last_pong);
            break WebsocketConnectionError::PongError();
        }
        if let Err(err) = write_to_socket_sender.send(Message::Ping("".into())).await {
            log::error!("Failed to send ping: {}", err);
            break WebsocketConnectionError::WriteClosed(err.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use tokio::time::Instant;

    #[tokio::test]
    async fn test_pong_check_sends_ping() {
        let mut interval = time::interval(Duration::from_millis(50));
        let last_pong = Arc::new(RwLock::new(Instant::now()));
        let (tx, mut rx) = mpsc::channel(1);

        let last_pong_clone = last_pong.clone();
        let handle = tokio::spawn(async move {
            pong_check_interval_task(&mut interval, &last_pong_clone, &tx).await
        });

        tokio::time::sleep(Duration::from_millis(60)).await;

        // Verify that a ping message was sent
        let message = rx.try_recv();
        assert!(matches!(message, Ok(Message::Ping(_))));

        handle.abort();
    }

    #[tokio::test]
    async fn test_pong_check_fails_on_no_pong() {
        let mut interval = time::interval(Duration::from_millis(50));
        let last_pong = Arc::new(RwLock::new(Instant::now() - Duration::from_secs(11))); // Too old
        let (tx, _) = mpsc::channel(1);

        let last_pong_clone = last_pong.clone();
        let handle = tokio::spawn(async move {
            pong_check_interval_task(&mut interval, &last_pong_clone, &tx).await
        });

        let outcome = handle.await.unwrap();
        assert!(matches!(outcome, WebsocketConnectionError::PongError()));
    }

    #[tokio::test]
    async fn test_pong_check_fails_when_write_closed() {
        let mut interval = time::interval(Duration::from_millis(50));
        let last_pong = Arc::new(RwLock::new(Instant::now()));
        let (tx, rx) = mpsc::channel(1);

        drop(rx); // Close the receiver

        let last_pong_clone = Arc::clone(&last_pong);
        let handle = tokio::spawn(async move {
            pong_check_interval_task(&mut interval, &last_pong_clone, &tx).await
        });

        let outcome = handle.await.unwrap();
        assert!(matches!(outcome, WebsocketConnectionError::WriteClosed(_)));
    }

}
