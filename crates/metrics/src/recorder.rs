#![allow(missing_docs)]

use eyre::WrapErr;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use metrics_util::layers::{PrefixLayer, Stack};
use std::sync::{atomic::AtomicBool, LazyLock};

pub fn install_prometheus_recorder() -> &'static PrometheusRecorder {
    &PROMETHEUS_RECORDER
}

static PROMETHEUS_RECORDER: LazyLock<PrometheusRecorder> =
    LazyLock::new(|| PrometheusRecorder::install().unwrap());

#[derive(Debug)]
pub struct PrometheusRecorder {
    handle: PrometheusHandle,
    upkeep: AtomicBool,
}

impl PrometheusRecorder {
    const fn new(handle: PrometheusHandle) -> Self {
        Self {
            handle,
            upkeep: AtomicBool::new(false),
        }
    }

    pub fn install() -> eyre::Result<Self> {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();

        Stack::new(recorder)
            .push(PrefixLayer::new("eth_kit"))
            .install()
            .wrap_err("failed to install metrics recorder")?;

        Ok(Self::new(handle))
    }

    pub fn spawn_upkeep(&self) {
        if self
            .upkeep
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_err()
        {
            return;
        }

        let handle = self.handle.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                handle.run_upkeep();
            }
        });
    }

    pub const fn handle(&self) -> &PrometheusHandle {
        &self.handle
    }
}
