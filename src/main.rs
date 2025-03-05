#![allow(warnings)]
mod file_watcher;
mod ws_server;
mod ws_client;

use std::ops::{Div, Sub};
use std::path::Path;
use std::sync::Arc;
use async_channel::Receiver;
use axum::extract::WebSocketUpgrade;
use axum::response::IntoResponse;
use axum::Router;
use axum::routing::{any, get};
use notify::Watcher;
use chrono::Duration;
use dotenv::dotenv;
use tokio::net::TcpListener;
use crate::file_watcher::FileWatcher;
use crate::ws_client::run_client;
use crate::ws_server::{run_server, upgrade_conn, WsServerState};

const TOLERANCE: Duration = Duration::milliseconds(100);

struct ArgSettings {
    is_client: bool,
    files_to_watch: Vec<String>,
    transfer_to: Option<String>,
}

#[tokio::main]
async fn main() {
    dotenv().unwrap();
    // "../cstroll/cmake-build-debug/cstroll.dll", "../cstroll/cmake-build-debug/libcstroll.dll.a"
    let mut settings = parse_args().await;
    if let Ok(t) = std::env::var("TRANSFER_TO") {
        settings.transfer_to = Some(t);
    }

    if let Ok(ff) = std::env::var("FROM_FILES") {
        settings.files_to_watch.extend(ff.split(",").map(|x| x.to_string()).collect::<Vec<String>>());
    }

    if !settings.is_client {
        run_server(settings.files_to_watch).await.unwrap();
        return;
    }

    match settings.transfer_to {
        None => {
            eprintln!("NO TRANSFER TO BUT CLIENT SELECTED");
        }
        Some(tt) => {
            run_client(tt).await.unwrap();
        }
    }
}

async fn parse_args() -> ArgSettings {
    let args = std::env::args().collect::<Vec<String>>();
    let mut for_files = false;

    let mut settings = ArgSettings {
        is_client: false,
        files_to_watch: vec![],
        transfer_to: None,
    };
    for arg in args.into_iter().skip(1) {
        if !for_files && arg == "client" {
            settings.is_client = true;
            break;
        }
        if arg == "-f".to_string() {
            for_files = true;
            continue;
        }
        if arg.starts_with("-") {
            for_files = false;
            continue;
        }
        if for_files {
            settings.files_to_watch.push(arg);
        }
    }
    settings
}

