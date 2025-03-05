use std::path::PathBuf;
use async_tungstenite::tokio::connect_async;
use async_tungstenite::tungstenite::Message;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::time::{sleep, timeout};

pub async fn run_client(transfer_to: String) -> anyhow::Result<()> {
    let (mut conn, _) = connect_async("ws://localhost:3827/ws").await?;
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
            _result = sleep(std::time::Duration::from_millis(msg_delay)) => {
                println!("SENDING HEARTBEAT");
                match write.send(Message::text("heartbeat")).await {
                    Err(e) => {
                        eprintln!("{e}");
                        break;
                    },
                    Ok(_) => {}
                }
                last_message = Utc::now().timestamp_millis();
            },
            result = read.next() => {
                let msg = match result {
                    None => {
                        eprintln!("reader closed");
                        break;
                    },
                    Some(Err(e)) => {
                        eprintln!("reader error: {e}");
                        continue;
                    }
                    Some(Ok(msg)) => msg
                };
                match msg {
                    Message::Binary(bytes) => {
                        let fl = bytes.to_vec();
                        let mut alt_len = 0;
                        let mut cut_at = 0usize;
                        for i in (0..fl.len()).rev() {
                            if fl[i] == alt_len % 2 {
                                alt_len += 1;
                            } else if fl[i] == 0 {
                                alt_len = 1;
                            } else {
                                alt_len = 0;
                            }

                            if alt_len == 8 {
                                cut_at = i+8;
                                break
                            }
                        }
                        if cut_at == 0 {
                            eprintln!("COULDN'T FIND NAME");
                            continue;
                        }
                        let (fl_conts, name) = fl.split_at(cut_at-8);
                        let name = String::from_utf8_lossy(name.split_at(8).1).to_string();

                        let w_to = transfer_to.join(name);
                        let fl_conts = fl_conts.to_vec();
                        tokio::spawn(async move {
                            match tokio::fs::write(w_to.clone(), fl_conts).await {
                                Err(e) => eprintln!("{e}"),
                                Ok(_) => {}
                            };
                        });
                    }
                    _ => {
                        println!("NON BINARY MSG {msg:?}");
                        continue;
                    }
                }

            }
        }
    }

    Ok(())
}
