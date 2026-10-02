use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::ConnectInfo;
use axum::response::IntoResponse as _;
use syntaxmesh_core::Node;
use syntaxmesh_ownership_host::OwnerEndpoint;

#[test]
fn bounded_successful_requests_reuse_the_same_tcp_connection()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempfile::tempdir()?;
    let database = fixture.path().join("graph.db");
    std::fs::write(&database, b"connection fixture")?;
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let endpoint = OwnerEndpoint::new(&database, listener.local_addr()?)?;
    let instance = endpoint.instance().to_owned();
    let connections = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&connections);
    let router = axum::Router::new().fallback(move |ConnectInfo(peer): ConnectInfo<SocketAddr>| {
        let observed = Arc::clone(&observed);
        let instance = instance.clone();
        async move {
            match observed.lock() {
                Ok(mut peers) => peers.push(peer),
                Err(_error) => {
                    return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
            let mut response = axum::Json(serde_json::json!({
                "schema_version": 1, "generation": "a".repeat(64), "data": []
            }))
            .into_response();
            match axum::http::HeaderValue::from_str(&instance) {
                Ok(value) => {
                    response
                        .headers_mut()
                        .insert("x-syntaxmesh-owner-instance", value);
                }
                Err(_error) => {
                    return axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
            response
        }
    });
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let worker = std::thread::spawn(move || -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())?;
        runtime
            .block_on(async {
                let listener = tokio::net::TcpListener::from_std(listener)?;
                axum::serve(
                    listener,
                    router.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .with_graceful_shutdown(async {
                    drop(stop_rx.await);
                })
                .await
            })
            .map_err(|error| error.to_string())
    });
    let result = (|| -> Result<(), super::CliError> {
        let client = super::Client::new(&endpoint)?;
        for _ in 0..2 {
            let (_, _nodes): (_, Vec<Node>) =
                client.get_data("/api/v1/search", &[("limit", "1")])?;
        }
        Ok(())
    })();
    let _stopped = stop_tx.send(());
    worker
        .join()
        .map_err(|_panic| "connection fixture panicked")??;
    result?;
    let peers = connections.lock().map_err(|error| error.to_string())?;
    if peers.len() != 2 || peers.first() != peers.last() {
        return Err("successful requests did not reuse one connection".into());
    }
    Ok(())
}
