#![allow(missing_docs)]

use alloy_primitives::U64;
use alloy_rpc_types_eth::{Block, SyncStatus};
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use jsonrpsee::core::client::ClientT;
use jsonrpsee::core::params::BatchRequestBuilder;
use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::rpc_params;
use metrics::{describe_gauge, gauge};
use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info};

async fn echo(
    req: Request<hyper::body::Incoming>,
    node_uri: String,
    min_peers: u16,
) -> eyre::Result<Response<BoxBody<Bytes, hyper::Error>>> {
    match (req.method(), req.uri().path()) {
        // Serve some instructions at /
        (&Method::GET, "/") => match is_healthy(node_uri, min_peers).await {
            Ok(_) => {
                gauge!("execution_node_status").set(1.0);
                Ok(Response::new(full("ok")))
            }
            Err(e) => {
                gauge!("execution_node_status").set(0.0);
                let mut response = Response::new(full(e.to_string()));
                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                Ok(response)
            }
        },

        // return 404 Not Found for other routes.
        _ => {
            let mut not_found = Response::new(empty());
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

async fn is_healthy(uri: String, min_peers: u16) -> eyre::Result<()> {
    let client = HttpClientBuilder::default().build(uri).unwrap();

    let mut batch = BatchRequestBuilder::new();
    batch.insert("eth_syncing", rpc_params![]).unwrap();
    batch.insert("net_peerCount", rpc_params![]).unwrap();
    batch
        .insert("eth_getBlockByNumber", rpc_params!["latest", false])
        .unwrap();

    let responses = client
        .batch_request(batch)
        .await
        .map_err(|e| eyre::eyre!("failed to send batch request: {:?}", e))?
        .into_ok()
        .map_err(|e| eyre::eyre!("failed to get batch response: {:?}", e))?;

    for (index, response) in responses.into_iter().enumerate() {
        match index {
            0 => {
                let syncing_status: SyncStatus = serde_json::from_value(response)?;
                match syncing_status {
                    SyncStatus::None => (),
                    SyncStatus::Info(ref info) => {
                        return Err(eyre::eyre!(
                            "node is syncing, current block {:?}, latest block {:?}",
                            info.current_block,
                            info.highest_block
                        ))
                    }
                }
            }
            1 => {
                let peer_count: U64 = serde_json::from_value(response)?;
                if peer_count < U64::from(min_peers + 1) {
                    return Err(eyre::eyre!(
                        "not enough peers min: {:?}, current: {:?}",
                        min_peers,
                        peer_count
                    ));
                }
            }
            2 => {
                let block_info: Block = serde_json::from_value(response)?;
                if block_info.header.number == 0 {
                    return Err(eyre::eyre!("no blocks synced yet"));
                }
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("time went backwards")
                    .as_secs();
                let time_diff = now - block_info.header.timestamp;
                if time_diff > 60 {
                    return Err(eyre::eyre!(
                        "latest block has not been updated for {:?}",
                        time_diff
                    ));
                }
            }
            _ => error!("unexpected response at index {}", index),
        }
    }

    Ok(())
}

fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

pub async fn serve_echo(addr: SocketAddr, node_uri: String, min_peers: u16) -> eyre::Result<()> {
    describe_gauge!("execution_node_status", "execution node status");

    let listener = TcpListener::bind(addr).await.unwrap();
    info!("echo listening on http://{}", addr);

    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();

    let connection_timeouts = vec![Duration::from_secs(5), Duration::from_secs(2)];

    loop {
        tokio::select! {
            incoming = listener.accept() => {
                let (tcp, _) = incoming?;
                let io = TokioIo::new(tcp);

                let node_uri_clone = node_uri.clone();
                let service = service_fn(move |req| echo(req, node_uri_clone.to_string(), min_peers));

                let connection_timeouts_clone = connection_timeouts.clone();

                tokio::task::spawn(async move {
                    let mut conn = Box::pin(http1::Builder::new().serve_connection(io, service));

                    for (iter, sleep_duration) in connection_timeouts_clone.iter().enumerate() {
                        tokio::select! {
                            res = conn.as_mut() => {
                                    match res {
                                        Ok(()) => (),
                                        Err(e) => info!("echo connection error: {:?}", e),
                                    }
                                    break;
                            }
                            _ = tokio::time::sleep(*sleep_duration) => {
                                        info!("iter = {} echo connection timed out", iter);
                                        conn.as_mut().graceful_shutdown();
                            }
                        }
                    }
                });
            },

            _ = sigint.recv() => {
                info!("received sigint, shutting down");
                break;
            }

            _ = sigterm.recv() => {
                info!("received sigterm, shutting down");
                break;
            }
        };
    }

    Ok(())
}
