//! Proves the bind-and-serve path, not just the router.
//!
//! Spoken over a raw socket rather than through an HTTP client: one dependency
//! fewer, and the request is simple enough to write by hand.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::test]
async fn serves_over_a_real_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let address = listener.local_addr().expect("has an address");
    let server = tokio::spawn(timemd_server::serve(listener));

    let mut stream = TcpStream::connect(address).await.expect("connects");
    stream
        .write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("sends the request");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .await
        .expect("reads the response");

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains(r#""status":"ok""#), "{response}");

    server.abort();
}
