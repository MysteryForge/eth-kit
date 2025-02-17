#![allow(missing_docs)]

use clap::Parser;
use std::net::{SocketAddr, ToSocketAddrs};
use url::Url;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(
        short,
        long,
        value_name = "ADDR",
        env = "ADDR",
        default_value = "0.0.0.0"
    )]
    addr: String,

    #[arg(short, long, value_name = "PORT", env = "PORT", default_value = "3031")]
    port: u16,

    #[arg(
        long,
        value_name = "METRICS_ADDR",
        env = "METRICS_ADDR",
        default_value = "0.0.0.0"
    )]
    metrics_addr: String,

    #[arg(
        long,
        value_name = "METRICS_PORT",
        env = "METRICS_PORT",
        default_value = "3001"
    )]
    metrics_port: u16,

    #[arg(long, value_name = "NODE_URI", env = "NODE_URI", required = true)]
    node_uri: String,

    #[arg(long, value_name = "MIN_PEERS", env = "MIN_PEERS", default_value = "2")]
    min_peers: u16,
}

impl Cli {
    pub fn resolve_addr(&self) -> eyre::Result<SocketAddr> {
        let addr_str = format!("{}:{}", self.addr, self.port);
        addr_str
            .to_socket_addrs()
            .map_err(|e| eyre::eyre!("failed to parse address {}: {}", addr_str, e))?
            .next()
            .ok_or_else(|| eyre::eyre!("unable to resolve address: {}", addr_str))
    }

    pub fn resolve_metrics_addr(&self) -> eyre::Result<SocketAddr> {
        let addr_str = format!("{}:{}", self.metrics_addr, self.metrics_port);
        addr_str
            .to_socket_addrs()
            .map_err(|e| eyre::eyre!("failed to parse address {}: {}", addr_str, e))?
            .next()
            .ok_or_else(|| eyre::eyre!("unable to resolve address: {}", addr_str))
    }

    pub fn resolve_node_uri(&self) -> eyre::Result<&str> {
        match Url::parse(&self.node_uri) {
            Ok(uri) => {
                if uri.scheme() == "http" || uri.scheme() == "https" {
                    Ok(&self.node_uri)
                } else {
                    Err(eyre::eyre!("invalid scheme: {:?}", uri.scheme()))
                }
            }
            Err(e) => Err(eyre::eyre!("failed to parse node uri: {}", e)),
        }
    }

    pub fn resolve_min_peers(&self) -> eyre::Result<u16> {
        if self.min_peers > 5 {
            return Err(eyre::eyre!("min_peers should be less than 5"));
        }
        Ok(self.min_peers)
    }
}
