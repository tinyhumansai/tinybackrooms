//! Unit tests for the backend stub.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

async fn exchange(url: &str, request: &str) -> String {
    let mut socket = TcpStream::connect(url.trim_start_matches("http://"))
        .await
        .unwrap();
    socket.write_all(request.as_bytes()).await.unwrap();
    let mut reply = String::new();
    socket.read_to_string(&mut reply).await.unwrap();
    reply
}

#[tokio::test]
async fn answers_a_get_with_the_success_body() {
    let (url, task) = spawn().await.unwrap();
    let reply = exchange(&url, "GET /anything HTTP/1.1\r\nHost: x\r\n\r\n").await;
    assert!(reply.starts_with("HTTP/1.1 200 OK"), "{reply}");
    assert!(reply.ends_with(BODY));
    task.abort();
}

#[tokio::test]
async fn drains_a_request_body_before_answering() {
    let (url, task) = spawn().await.unwrap();
    let body = "x".repeat(10_000);
    let request = format!(
        "POST /v1/thing HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let reply = exchange(&url, &request).await;
    assert!(reply.contains("\"success\":true"), "{reply}");
    task.abort();
}

#[tokio::test]
async fn survives_a_client_that_hangs_up_early() {
    let (url, task) = spawn().await.unwrap();
    drop(
        TcpStream::connect(url.trim_start_matches("http://"))
            .await
            .unwrap(),
    );
    let mut half = TcpStream::connect(url.trim_start_matches("http://"))
        .await
        .unwrap();
    half.write_all(b"POST / HTTP/1.1\r\nContent-Length: 50\r\n\r\nshort")
        .await
        .unwrap();
    drop(half);
    // Still serving afterwards.
    let reply = exchange(&url, "GET / HTTP/1.1\r\nHost: x\r\n\r\n").await;
    assert!(reply.starts_with("HTTP/1.1 200"));
    task.abort();
}

#[test]
fn reads_the_content_length_case_insensitively() {
    assert_eq!(content_length(b"POST / HTTP/1.1\r\nCONTENT-LENGTH: 12"), 12);
    assert_eq!(content_length(b"GET / HTTP/1.1\r\nHost: x"), 0);
    assert_eq!(
        content_length(b"POST / HTTP/1.1\r\ncontent-length: nope"),
        0
    );
}
