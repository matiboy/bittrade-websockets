use tokio::net::UnixStream;
use tokio::select;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;
use tokio::{net::UnixListener, sync::broadcast};
use tokio::io::AsyncWriteExt;



pub async fn create_socket(path: &String, to_sockets_messages_sender: &broadcast::Sender<String>, to_sockets_messages_receiver: &mut broadcast::Receiver<String>) -> Result<(), UnixSocketError> {
    loop {
        if std::fs::exists(path)? {
            log::warn!("Removing existing unix socket file: {}", path);
            if let Err(err) = std::fs::remove_file(path) {
                log::error!("Failed to remove existing unix socket file: {}", err);
                return Err(UnixSocketError::Fatal(format!("Failed to remove existing unix socket file: {}", err)));
            }
        }
        let (current_value_tx, current_value_rx) = watch::channel::<Option<String>>(None);
        let socket = match UnixListener::bind(path) {
            Ok(socket) => socket,
            Err(err) => {
                log::error!("Failed to bind to unix socket {}: {}", path, err);
                return Err(UnixSocketError::ConnectionError(format!("Failed to bind to unix socket {}: {}", path, err)));
            }
        };
        // let (to_sockets_messages_sender, _) = broadcast::channel::<String>(32);
        select! {
            _ = accept_connections(socket, to_sockets_messages_sender, current_value_rx) => {
                log::error!("Failed to accept connections");
            }
            _ = keep_latest_value(to_sockets_messages_receiver, &current_value_tx) => {
                log::error!("Failed to keep latest value");
            }
        }
    }
}

async fn keep_latest_value(to_sockets_messages_receiver: &mut broadcast::Receiver<String>, current_value_tx: &watch::Sender<Option<String>>) -> () {
    loop {
        match to_sockets_messages_receiver.recv().await {
            Ok(message) => {
                log::trace!("Received message: {}", message);
                current_value_tx.send(Some(message)).expect("Failed to send message");
            }
            Err(broadcast::error::RecvError::Lagged(err)) => {
                log::error!("Lag in broadcasting message: {err}");
            }
            Err(broadcast::error::RecvError::Closed) => {
                log::error!("Channel closed");
                break;
            }
        }
    }
}

async fn accept_connections(socket: UnixListener, to_sockets_messages_sender: &broadcast::Sender<String>, current_value_rx: watch::Receiver<Option<String>>) {
    loop {
        match socket.accept().await {
            Ok((stream, _)) => {
                // TODO should we keep these join handles and abort them on error or other termination conditions?
                handle_single_connection(stream, to_sockets_messages_sender, &current_value_rx);
            }
            Err(err) => {
                log::warn!("Failed to accept connection: {}", err);
            }
        }
    }
}

fn handle_single_connection(mut stream: UnixStream, to_sockets_messages_sender: &broadcast::Sender<String>, current_value_rx: &watch::Receiver<Option<String>>) {
    log::debug!("Accepted connection");
    let mut receiver = to_sockets_messages_sender.subscribe();
    let latest_message = current_value_rx.borrow().as_ref()
        .map(|s| s.clone().into_bytes());
    tokio::spawn(async move {
        if let Some(message) = latest_message {
            if let Err(err) = stream.write_all(&message).await {
                log::warn!("Failed to write; Unix socket client likely disconnected");
                log::debug!("Failed to write to socket: {}", err);
                return;
            }
        }
        if let Err(err) = handle_connection_messages(&mut stream, &mut receiver).await {
            log::error!("Failed to receive messages on broadcast channel: {}", err);
        }
    });
}

async fn handle_connection_messages(stream: &mut UnixStream, receiver: &mut broadcast::Receiver<String>) -> Result<(), RecvError> {
    loop {
        match receiver.recv().await {
            Ok(message) => {
                log::trace!("Message being sent on unix socket: {}", message);
                if let Err(err) = stream.write_all(message.as_bytes()).await {
                    log::warn!("Failed to write; Unix socket client likely disconnected");
                    log::debug!("Failed to write to socket: {}", err);
                    break Ok(())
                }
            }
            Err(RecvError::Lagged(err)) => {
                log::warn!("Lag in broadcasting message: {err}");
            }
            Err(err) => {
                log::error!("Error receiving message: {}", err);
                break Err(err);
            }
        }
    }
}

pub enum UnixSocketError {
    Fatal(String),
    ConnectionError(String),
}

impl From<std::io::Error> for UnixSocketError {
    fn from(err: std::io::Error) -> Self {
        UnixSocketError::ConnectionError(format!("{}", err))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::{AsyncBufReadExt, BufReader};

    #[tokio::test]
    async fn test_handle_connection_messages_disconnect() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test_socket");

        let listener = UnixListener::bind(&socket_path).unwrap();
        let (tx, mut rx) = broadcast::channel(10);

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            handle_connection_messages(&mut stream, &mut rx).await
        });

        // Create a client connection and then drop it
        let client_stream = UnixStream::connect(&socket_path).await.unwrap();
        drop(client_stream); // Simulating client disconnection
        
        // Send a message, should cause an error in writing
        tx.send("This should fail".to_string()).unwrap();
        
        // Ensure the task completes
        let outcome = handle.await.unwrap();
        assert!(outcome.is_ok());
    }

    #[tokio::test]
    async fn test_handle_connection_messages_broadcast_channel_fail() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test_socket");

        let listener = UnixListener::bind(&socket_path).unwrap();
        let (tx, mut rx) = broadcast::channel(10);

        drop(tx); // Simulating that the only receiver has been dropped
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            handle_connection_messages(&mut stream, &mut rx).await
        });

        // Create a client connection and then drop it; if we don't have a client, the task will not try to send to the receiver and thus it won't fail
        let _client_stream = UnixStream::connect(&socket_path).await.unwrap();
        
        // Ensure the task completes
        let outcome = handle.await.unwrap();
        assert!(outcome.is_err());
        assert!(matches!(outcome.err().unwrap(), RecvError::Closed));
    }

    #[tokio::test]
    async fn test_handle_connection_messages_broadcast_channel_lag_is_acceptable() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test_socket");

        let listener = UnixListener::bind(&socket_path).unwrap();
        let (tx, mut rx) = broadcast::channel(1);

        tx.send("ABC\n".to_string()).unwrap();
        tx.send("DEF\n".to_string()).unwrap();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            handle_connection_messages(&mut stream, &mut rx).await
        });

        // Create a client connection and then drop it; if we don't have a client, the task will not try to send to the receiver and thus it won't fail
        let mut client_stream = UnixStream::connect(&socket_path).await.unwrap();

        // Read messages from the client stream
        let reader = BufReader::new(&mut client_stream);
        let mut received_messages = Vec::<String>::new();
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            received_messages.push(line);
            if received_messages.len() == 1 {
                break;
            }
        }
        // Disconnect and try to send so we terminate the task
        drop(client_stream);
        tx.send("GHI\n".to_string()).unwrap();
        // Ensure the task completes despite the lag
        let outcome = handle.await.unwrap();
        assert!(outcome.is_ok());
        assert!(received_messages.len() == 1);
        // Due to the buffer size of 1 we expect to only see the second message
        assert_eq!(received_messages.get(0).unwrap(), "DEF");
    }


    #[tokio::test]
    async fn test_keep_latest_value_updates_watch_channel() {
        let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel(10);
        let (watch_tx, watch_rx) = tokio::sync::watch::channel(None);

        let handle = tokio::spawn(async move {
            keep_latest_value(&mut broadcast_rx, &watch_tx).await;
        });
        // Send a message
        broadcast_tx.send("Hello".to_string()).unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        handle.abort();
        // Ensure watch received the latest value
        assert_eq!(*watch_rx.borrow(), Some("Hello".to_string()));
    }

    // #[tokio::test]
    // async fn test_keep_latest_value_handles_lagged_messages() {
    //     let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel(2);
    //     let (watch_tx, watch_rx) = tokio::sync::watch::channel(None);

    //     let handle = tokio::spawn(keep_latest_value(&mut broadcast_rx, &watch_tx));

    //     // Send multiple messages to overflow receiver
    //     for i in 0..5 {
    //         let _ = broadcast_tx.send(format!("Message {}", i));
    //     }

    //     tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    //     // The latest valid message should be the last one
    //     assert_eq!(*watch_rx.borrow(), Some("Message 4".to_string()));

    //     handle.abort();
    // }

    // #[tokio::test]
    // async fn test_keep_latest_value_stops_when_channel_closes() {
    //     let (broadcast_tx, mut broadcast_rx) = tokio::sync::broadcast::channel(2);
    //     let (watch_tx, _watch_rx) = tokio::sync::watch::channel(None);

    //     let handle = tokio::spawn(keep_latest_value(&mut broadcast_rx, &watch_tx));

    //     // Drop the sender, closing the channel
    //     drop(broadcast_tx);

    //     tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    //     // The task should have exited
    //     assert!(handle.is_finished());
    // }

}
