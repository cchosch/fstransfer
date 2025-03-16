use std::collections::HashMap;
use std::ffi::OsStr;
use std::ops::{Div, Sub};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use anyhow::anyhow;
use async_channel::{Receiver, Sender};
use chrono::Utc;
use log::{error, info};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use path_absolutize::Absolutize;
use tokio::sync::Mutex;
use tokio::time::interval;
use crate::TOLERANCE;

pub struct FileWatcher {
    file_paths: Vec<PathBuf>,
    pub receiver: Receiver<(PathBuf, EventKind)>
}

impl FileWatcher {
    pub async fn new<T: Into<PathBuf>>(file_path: Vec<T>) -> anyhow::Result<FileWatcher> {
        let (s, r) = async_channel::unbounded::<(PathBuf, EventKind)>();
        let file_paths = file_path
            .into_iter()
            .map(|x| x.into().absolutize().map(|x| x.to_path_buf()).ok())
            .filter(|x| x.is_some())
            .map(|x| x.unwrap())
            .collect::<Vec<PathBuf>>();

        let file_path = get_common_parent(&file_paths)?;

        tokio::spawn(Self::run_loop(file_path.clone(), s.clone()));
        Ok(Self {
            file_paths,
            receiver: r,
        })
    }


    /// Gets event from receiver. If channel closed, return [`None`]
    pub async fn get_event(&self) -> Option<(PathBuf, EventKind)>{
        while let (p, e) = self.receiver.recv().await.ok()? {
            let p = match p.absolutize() {
                Err(_) => continue,
                Ok(p) => p.to_path_buf(),
            };
            for file_paths in self.file_paths.iter() {
                if &p == file_paths {
                    return Some((p, e));
                }
            }
        }

        unreachable!();
    }

    pub async fn run_loop(file_path: PathBuf, sender: Sender<(PathBuf, EventKind)>) {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
        let (send_file, recv_file) = async_channel::unbounded::<(PathBuf, EventKind)>();

        let mut watcher = notify::recommended_watcher(tx).unwrap();
        watcher.watch(file_path.as_path(), RecursiveMode::Recursive).unwrap();

        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            for item in rx {
                if let Ok(event) = item {
                    for path in event.paths {
                        let new_sender = send_file.clone();
                        let kind = event.kind.clone();
                        handle.spawn(async move {
                            new_sender.send((path, kind)).await;
                        });
                    }
                }
            }
            info!("File watcher stopped");
        });

        let mut file_tracker = Arc::new(Mutex::new(HashMap::<PathBuf, (chrono::DateTime<Utc>, EventKind)>::new()));

        let local_fl_tracker = file_tracker.clone();
        let l_recv = recv_file.clone();
        tokio::spawn(async move {
            while let Ok((fl, kind)) = l_recv.recv().await {
                let mut fl_t = local_fl_tracker.lock().await;
                fl_t.insert(fl, (Utc::now(), kind));
            }
        });

        let mut int = interval(TOLERANCE.div(2).to_std().unwrap());
        while !recv_file.is_closed() {
            int.tick().await;

            let mut fl_t = file_tracker.lock().await;
            let mut to_remove = vec![];
            for (file_name, (file_time, event_kind)) in fl_t.iter(){
                if Utc::now() - file_time > TOLERANCE {
                    to_remove.push(file_name.clone());
                }
            }

            for fl_name in to_remove {
                let msg = match fl_t.remove(&fl_name) {
                    None => continue,
                    Some((_, event_kind)) => (fl_name, event_kind)
                };

                if let Err(_e) = sender.send(msg).await {
                    break;
                }

            }
        }

        info!("Done");

    }
}

fn get_common_parent(file_paths: &Vec<PathBuf>)-> anyhow::Result<PathBuf> {
    let mut parents = vec![];
    for file_path in file_paths {
        let mut file_path = match file_path.absolutize() {
            Err(e) => {
                error!("{e} -> {file_path:?}");
                continue
            },
            Ok(p) => p.to_path_buf()
        };
        file_path.pop();
        parents.push(file_path);
    }

    if parents.len() == 0 {
        return Err(anyhow!("No parent found"));
    }

    let mut common: Vec<&OsStr> = vec![];
    let mut i = 0;
    let mut max_len = 1;
    loop {
        if parents.len() == 1 {
            return Ok(parents[0].clone());
        }

        let v = parents[0].iter().collect::<Vec<&OsStr>>();
        max_len = v.len();
        let c_level = v[i];
        let mut finish = false;
        for p in parents.iter().skip(1) {
            let lv = p.iter().collect::<Vec<&OsStr>>();
            max_len = max_len.min(lv.len());
            if lv[i] != c_level {
                finish = true;
                break;
            }
        }
        if finish {
            break;
        }
        common.push(c_level);
        i+=1;
        if i == max_len {
            break;
        }
    }
    let mut path_buf = PathBuf::new();
    for component in common {
        path_buf.push(component);
    }
    return Ok(path_buf);
}
