use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[allow(dead_code)]
pub struct MockWsServer {
    pub url: String,
    cmd_tx: mpsc::Sender<WsCommand>,
    pub received_rx: mpsc::Receiver<String>,
}

#[allow(dead_code)]
enum WsCommand {
    SendText(String),
    SendPing(Vec<u8>),
    Close,
}

impl MockWsServer {
    /// Start.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("ws://127.0.0.1:{port}");

        let (cmd_tx, mut cmd_rx) = mpsc::channel::<WsCommand>(64);
        let (received_tx, received_rx) = mpsc::channel::<String>(64);

        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                    continue;
                };
                let (mut write, mut read) = ws.split();

                loop {
                    tokio::select! {
                        cmd = cmd_rx.recv() => {
                            match cmd {
                                Some(WsCommand::SendText(text)) => {
                                    let _ = write.send(Message::Text(text.into())).await;
                                }
                                Some(WsCommand::SendPing(data)) => {
                                    let _ = write.send(Message::Ping(data.into())).await;
                                }
                                Some(WsCommand::Close) => {
                                    let _ = write.send(Message::Close(None)).await;
                                    break;
                                }
                                None => return,
                            }
                        }
                        msg = read.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    let _ = received_tx.send(text.to_string()).await;
                                }
                                Some(Ok(Message::Close(_))) | None => break,
                                _ => {}
                            }
                        }
                    }
                }
            }
        });

        Self {
            url,
            cmd_tx,
            received_rx,
        }
    }

    /// Send.
    pub async fn send(&self, text: &str) {
        self.cmd_tx
            .send(WsCommand::SendText(text.to_string()))
            .await
            .unwrap();
    }

    /// Sends ping.
    #[allow(dead_code)]
    pub async fn send_ping(&self) {
        self.cmd_tx.send(WsCommand::SendPing(vec![])).await.unwrap();
    }

    /// Close.
    #[allow(dead_code)]
    pub async fn close(&self) {
        self.cmd_tx.send(WsCommand::Close).await.unwrap();
    }

    /// Wait for the client to send a message (e.g., subscription).
    #[allow(dead_code)]
    pub async fn recv(&mut self) -> Option<String> {
        self.received_rx.recv().await
    }
}
