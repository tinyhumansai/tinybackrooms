//! A local stand-in for the `TinyHumans` backend.
//!
//! `OpenHuman` makes a few non-inference calls to its backend, and signed out of
//! the real one they hang rather than fail. Bring-your-own-key runs never use
//! the real backend, so the runtime is pointed at this: a loopback server that
//! answers every request with the same small success body.

use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

const BODY: &str = r#"{"success":true,"data":{"id":"local","email":"local@openhuman.local"}}"#;

/// Start the stub on a free loopback port. Returns its base URL and the task
/// serving it, which the caller aborts to stop it.
pub(crate) async fn spawn() -> io::Result<(String, JoinHandle<()>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", listener.local_addr()?);
    let task = tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let _ = respond(socket).await;
            });
        }
    });
    Ok((url, task))
}

/// Read one request in full, then answer it and close.
async fn respond(mut socket: TcpStream) -> io::Result<()> {
    let mut received = Vec::new();
    let mut chunk = [0_u8; 4096];
    let (head_len, body_len) = loop {
        let read = socket.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        received.extend_from_slice(&chunk[..read]);
        if let Some(end) = received.windows(4).position(|window| window == b"\r\n\r\n") {
            break (end + 4, content_length(&received[..end]));
        }
    };
    // Unread request bytes at close can reset the connection before the client
    // reads the response, so drain the body first.
    while received.len() < head_len + body_len {
        let read = socket.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        received.extend_from_slice(&chunk[..read]);
    }
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
         connection: close\r\n\r\n{BODY}",
        BODY.len()
    );
    socket.write_all(response.as_bytes()).await?;
    socket.shutdown().await
}

fn content_length(head: &[u8]) -> usize {
    String::from_utf8_lossy(head)
        .to_lowercase()
        .lines()
        .find_map(|line| {
            line.strip_prefix("content-length:")
                .map(str::trim)
                .map(str::to_owned)
        })
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod test;
