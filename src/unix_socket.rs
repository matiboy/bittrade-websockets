use tokio::select;
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
                log::debug!("Received message: {}", message);
                current_value_tx.send(Some(message)).expect("Failed to send message");
            }
            Err(err) => {
                log::error!("Error receiving message: {}", err);
                break;
            }
        }
    }
}

async fn accept_connections(socket: UnixListener, to_sockets_messages_sender: &broadcast::Sender<String>, current_value_rx: watch::Receiver<Option<String>>) {
    loop {
        match socket.accept().await {
            Ok((mut stream, _)) => {
                log::debug!("Accepted connection");
                let mut receiver = to_sockets_messages_sender.subscribe();
                let latest_message = current_value_rx.borrow().as_ref()
                    .map(|s| format!("{}\n", s))
                    .map(|s| s.into_bytes());
                tokio::spawn(async move {
                    if let Some(message) = latest_message {
                        if let Err(err) = stream.write_all(&message).await {
                            log::warn!("Failed to write; Unix socket client likely disconnected");
                            log::debug!("Failed to write to socket: {}", err);
                            return;
                        }
                    }
                    loop {
                        match receiver.recv().await {
                            Ok(message) => {
                                let message = format!("{}\n", message);
                                log::trace!("Message being sent on unix socket: {}", message);
                                if let Err(err) = stream.write_all(message.as_bytes()).await {
                                    log::warn!("Failed to write; Unix socket client likely disconnected");
                                    log::debug!("Failed to write to socket: {}", err);
                                    break;
                                }
                            }
                            Err(err) => {
                                log::error!("Error receiving message: {}", err);
                                break;
                            }
                        } 
                    }
                });
            }
            Err(err) => {
                log::warn!("Failed to accept connection: {}", err);
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