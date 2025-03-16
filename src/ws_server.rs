use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::fmt::Debug;
use anyhow::Context;
use async_channel::{Receiver, Sender};
use axum::body::Bytes;
use axum::extract::{State, WebSocketUpgrade};
use axum::extract::ws::{Message, WebSocket};
use axum::response::IntoResponse;
use axum::Router;
use axum::routing::any;
use log::{error, info};
use tokio::net::TcpListener;
use tokio::time::timeout;
use crate::file_watcher::FileWatcher;

#[derive(Clone)]
pub struct WsServerState {
    recv: Receiver<Arc<Vec<u8>>>
}

pub async fn run_server<T: Into<PathBuf> + Debug>(to_watch: Vec<T>) -> anyhow::Result<()> {
    let fl_watcher = FileWatcher::new(to_watch).await.context("Failed to initialize FileWatcher")?;

    let (ws_state, sender) = WsServerState::new();
    let app = Router::new().fallback(upgrade_conn).with_state(ws_state.clone());

    let listener = TcpListener::bind("0.0.0.0:3827").await.context("Failed to start server")?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!("{e}");
        }
        ws_state.recv.close()
    });
    info!("server running on port 3827");

    loop {
        let (x, y) = match fl_watcher.get_event().await {
            None => break,
            Some(x) => x
        };
        info!("{x:?}, {y:?}");
        let mut fl_name = x
            .file_name()
            .map(|x| x.to_str())
            .flatten();

        let mut fl_name = match fl_name {
            None => {
                error!("INVALID FILE NAME, {x:?}");
                continue;
            },
            Some(fl_name) => fl_name.as_bytes().to_vec()
        };
        let name_len: [u8; 8] = fl_name.len().to_be_bytes();

        fl_name.extend_from_slice(&name_len[..]);

        let mut file_conts = match tokio::fs::read(x).await {
            Err(e) => {
                error!("{e}");
                continue
            },
            Ok(fl_cont) => fl_cont
        };
        file_conts.extend(fl_name);
        let file_conts = Arc::new(file_conts);

        match sender.send(file_conts).await {
            Err(e) => {
                error!("{e}");
                break
            },
            Ok(_) => {
            }
        };
    }

    Ok(())
}

pub async fn upgrade_conn(ws: WebSocketUpgrade, state: State<WsServerState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws_conn(socket, state.0))
}

async fn handle_ws_conn(mut socket: WebSocket, state: WsServerState) {
    loop {
        tokio::select! {
            result = timeout(Duration::from_secs(60), socket.recv()) => {
                let _msg = match result {
                    Err(_) | Ok(None) => break,
                    Ok(Some(Err(e))) => {
                        error!("{e}");
                        continue;
                    }
                    Ok(Some(Ok(msg))) => msg
                };
            },
            result = state.recv.recv() => {
                match result {
                    Err(e) => {
                        error!("{e}");
                        break;
                    },
                    Ok(x) => {
                        info!("SENDING...");
                        match socket.send(Message::Binary(Bytes::copy_from_slice(&x))).await {
                            Err(e) => break,
                            Ok(_) => {}
                        }
                    }
                }
            }
        }
    }

    info!("DONE WITH HANDLE CONN...");
}

impl WsServerState {
    pub fn new() -> (Self, Sender<Arc<Vec<u8>>>) {
        let (tx, rx) = async_channel::unbounded();

        (Self { recv: rx }, tx)
    }
}
