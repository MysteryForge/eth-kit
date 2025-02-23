#![allow(missing_docs)]

use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use metrics::{describe_gauge, gauge};
use reqwest::get;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info};

async fn echo(
    req: Request<hyper::body::Incoming>,
    node_uri: String,
) -> eyre::Result<Response<BoxBody<Bytes, hyper::Error>>> {
    match (req.method(), req.uri().path()) {
        // Serve some instructions at /
        (&Method::GET, "/") => match is_healthy(node_uri).await {
            Ok(_) => {
                gauge!("beacon_node_status").set(1.0);
                Ok(Response::new(full("ok")))
            }
            Err(e) => {
                gauge!("beacon_node_status").set(0.0);
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

async fn is_healthy(uri: String) -> eyre::Result<()> {
    let res = get(format!("{}/eth/v1/node/health", uri))
        .await
        .map_err(|e| eyre::eyre!("beacon node health check failed, {:?}", e))?;
    let status = res.status();
    let data = res
        .text()
        .await
        .map_err(|e| eyre::eyre!("beacon node health check failed, {:?}", e))?;

    if status != 200 {
        error!(
            "beacon node health check failed: status code {:?}, body: {:?}",
            status, data
        );
        return Err(eyre::eyre!(
            "beacon node health check failed: status code {:?}",
            status
        ));
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

pub async fn serve_echo(addr: SocketAddr, node_uri: String) -> eyre::Result<()> {
    describe_gauge!("beacon_node_status", "beacon node status");

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
                let service = service_fn(move |req| echo(req, node_uri_clone.to_string()));

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
