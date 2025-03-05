use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::fmt::Debug;
use async_channel::{Receiver, Sender};
use axum::body::Bytes;
use axum::extract::{State, WebSocketUpgrade};
use axum::extract::ws::{Message, WebSocket};
use axum::response::IntoResponse;
use axum::Router;
use axum::routing::any;
use tokio::net::TcpListener;
use tokio::time::timeout;
use crate::file_watcher::FileWatcher;

#[derive(Clone)]
pub struct WsServerState {
    recv: Receiver<Arc<Vec<u8>>>
}

pub async fn run_server<T: Into<PathBuf> + Debug>(to_watch: Vec<T>) -> anyhow::Result<()> {
    let fl_watcher = FileWatcher::new(to_watch).await.unwrap();

    let (ws_state, sender) = WsServerState::new();
    let app = Router::new().route("/{*x}", any(upgrade_conn)).with_state(ws_state);

    let listener = TcpListener::bind("0.0.0.0:3827").await.unwrap();
    tokio::spawn(async move {axum::serve(listener, app).await.unwrap();});

    loop {
        let (x, y) = fl_watcher.get_event().await.unwrap();
        println!("{x:?}, {y:?}");
        let mut fl_name = x.file_name().unwrap().to_str().unwrap().as_bytes().to_vec();
        for i in 0..8 {
            fl_name.insert(0, i % 2);
        }

        let mut file_conts = match tokio::fs::read(x).await {
            Err(e) => {eprintln!("{e}"); continue},
            Ok(fl_cont) => fl_cont
        };
        file_conts.extend(fl_name);
        let file_conts = Arc::new(file_conts);

        match sender.send(file_conts).await {
            Err(e) => {
                eprintln!("{e}");
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
                        eprintln!("{e}");
                        continue;
                    }
                    Ok(Some(Ok(msg))) => msg
                };
            },
            result = state.recv.recv() => {
                match result {
                    Err(e) => {
                        eprintln!("{e}");
                        break;
                    },
                    Ok(x) => {
                        println!("SENDING...");
                        match socket.send(Message::Binary(Bytes::copy_from_slice(&x))).await {
                            Err(e) => break,
                            Ok(_) => {}
                        }
                    }
                }
            }
        }
    }

    println!("DONE WITH HANDLE CONN...");
}

impl WsServerState {
    pub fn new() -> (Self, Sender<Arc<Vec<u8>>>) {
        let (tx, rx) = async_channel::unbounded();

        (Self { recv: rx }, tx)
    }
}
