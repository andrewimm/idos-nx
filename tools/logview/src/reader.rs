use std::time::Instant;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use crate::parse::{parse_log_line, LogEntry};

pub enum Source {
    Stdin,
    Socket(String),
}

pub async fn spawn_reader(source: Source, tx: mpsc::UnboundedSender<LogEntry>) {
    match source {
        Source::Stdin => read_lines(BufReader::new(tokio::io::stdin()), tx).await,
        Source::Socket(path) => {
            let stream = match tokio::net::UnixStream::connect(&path).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(LogEntry {
                        tag: String::new(),
                        color: 0,
                        message: format!("Failed to connect to {path}: {e}"),
                        timestamp: Instant::now(),
                    });
                    return;
                }
            };
            read_lines(BufReader::new(stream), tx).await;
        }
    }
}

async fn read_lines<R: tokio::io::AsyncRead + Unpin>(reader: BufReader<R>, tx: mpsc::UnboundedSender<LogEntry>) {
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if tx.send(parse_log_line(&line, Instant::now())).is_err() {
            break;
        }
    }
}
