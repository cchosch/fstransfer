use std::path::PathBuf;
use async_tungstenite::tokio::connect_async;
use async_tungstenite::tungstenite::Message;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use tokio::time::{sleep, timeout};

pub async fn run_client(transfer_to: String, host_ip: String) -> anyhow::Result<()> {
    let (mut conn, _) = connect_async(format!("ws://{host_ip}:3827/ws")).await?;
    let (mut write, mut read) = conn.split();
    let mut last_message = Utc::now().timestamp_millis();
    let transfer_to = PathBuf::from(transfer_to);

    loop {
        let mut msg_delay = 20 * 1000 - (Utc::now().timestamp_millis() - last_message);
        if msg_delay < 0 {
            msg_delay = 0;
        }
        let msg_delay = msg_delay as u64;

        tokio::select! {
            // heartbeat loop
            _result = sleep(std::time::Duration::from_millis(msg_delay)) => {
                info!("SENDING HEARTBEAT");
                match write.send(Message::text("heartbeat")).await {
                    Err(e) => {
                        error!("{e}");
                        break;
                    },
                    Ok(_) => {}
                }
                last_message = Utc::now().timestamp_millis();
            },
            // file recv loop
            result = read.next() => {
                let msg = match result {
                    None => {
                        error!("reader closed");
                        break;
                    },
                    Some(Err(e)) => {
                        error!("reader error: {e}");
                        continue;
                    }
                    Some(Ok(msg)) => msg
                };
                match msg {
                    /// MESSAGES ENCODING AS FOLLOWING
                    /// --FILE CONTENTS (variable width)-- --FILE NAME (x len)-- --NAME LENGTH (8 bytes)--
                    Message::Binary(bytes) => {
                        let fl = bytes.to_vec();
                        // separate contents from name length and extract name len
                        let (fl_conts, name_len_bytes) = fl.split_at(fl.len()-8);
                        let name_len_bytes: [u8; 8] = match name_len_bytes.try_into() {
                            Err(e) => {
                                error!("{e}");
                                continue;
                            },
                            Ok(x) => x
                        };
                        let name_len = usize::from_be_bytes(name_len_bytes);

                        // separate contents from name itself
                        let (fl_conts, name) = fl_conts.split_at(fl_conts.len()-name_len);
                        let name = String::from_utf8_lossy(name).to_string();

                        let w_to = transfer_to.join(name);
                        let fl_conts = fl_conts.to_vec();
                        tokio::spawn(async move {
                            match tokio::fs::write(w_to.clone(), fl_conts).await {
                                Err(e) => error!("{e}"),
                                Ok(_) => {}
                            };
                        });
                    }
                    _ => {
                        info!("NON BINARY MSG {msg:?}");
                        continue;
                    }
                }

            }
        }
    }

    Ok(())
}
