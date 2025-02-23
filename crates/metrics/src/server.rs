#![allow(missing_docs)]

use crate::recorder::install_prometheus_recorder;

use eyre::WrapErr;
use http::{header::CONTENT_TYPE, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::Response;
use hyper_util::rt::TokioIo;
use metrics_process::Collector;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::pin;
use tokio::signal::unix::{signal, SignalKind};

use tracing::info;

#[derive(Debug)]
pub struct MetricsServer {
    addr: SocketAddr,
}

impl MetricsServer {
    pub const fn new(addr: SocketAddr) -> Self {
        Self { addr }
    }

    pub async fn run(self) -> eyre::Result<()> {
        self.start_server()
            .await
            .wrap_err("failed to start metrics server")?;
        Collector::default().describe();
        Ok(())
    }

    async fn start_server(&self) -> eyre::Result<()> {
        let listener = TcpListener::bind(self.addr).await.unwrap();
        info!("metrics listening on http://{}", self.addr);

        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        let mut sigterm = signal(SignalKind::terminate()).unwrap();

        let connection_timeouts = vec![Duration::from_secs(5), Duration::from_secs(2)];

        loop {
            tokio::select! {
                incoming = listener.accept() => {
                    let (tcp, _) = incoming?;
                    let io = TokioIo::new(tcp);

                    let handle = install_prometheus_recorder();
                    let service = service_fn(move |_req| {
                        let metrics = handle.handle().render();
                        let mut response = Response::new(metrics);
                        response
                            .headers_mut()
                            .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain"));
                        async move { Ok::<_, Infallible>(response) }
                    });

                    let connection_timeouts_clone = connection_timeouts.clone();

                    tokio::task::spawn(async move {
                        let conn = http1::Builder::new().serve_connection(io, service);
                        pin!(conn);

                        for (iter, sleep_duration) in connection_timeouts_clone.iter().enumerate() {
                            tokio::select! {
                                res = conn.as_mut() => {
                                        match res {
                                            Ok(()) => (),
                                            Err(e) => info!("metrics connection error: {:?}", e),
                                        }
                                        break;
                                }
                                _ = tokio::time::sleep(*sleep_duration) => {
                                            info!("iter = {} metrics connection timed out", iter);
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
}
