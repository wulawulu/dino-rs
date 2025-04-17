use anyhow::Result;
use clap::Parser;
use notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, new_debouncer};
use std::{fs, path::Path, time::Duration};
use tokio::sync::mpsc::channel;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tracing::{info, level_filters::LevelFilter, warn};
use tracing_subscriber::{Layer as _, fmt::Layer, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{CmdExecutor, utils::build_project};
use dino_server::{ProjectConfig, SwappableAppRouter, TenantRouter, start_server};

const MONITOR_FS_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Parser)]
pub struct RunOpts {}

impl CmdExecutor for RunOpts {
    async fn execute(self) -> anyhow::Result<()> {
        let layer = Layer::new().with_filter(LevelFilter::INFO);
        tracing_subscriber::registry().with(layer).init();

        let (code, config) = get_code_and_config()?;

        let router = SwappableAppRouter::try_new(&code, config.routes)?;

        tokio::spawn(async_watch(".", router.clone()));

        start_server(
            8888,
            vec![TenantRouter::new("localhost".to_string(), router)],
        )
        .await?;
        Ok(())
    }
}

fn get_code_and_config() -> Result<(String, ProjectConfig)> {
    let filename = build_project(".")?;
    let config = filename.replace(".mjs", ".yml");
    let code = fs::read_to_string(filename)?;
    let config = ProjectConfig::load(config)?;
    Ok((code, config))
}

async fn async_watch(path: impl AsRef<Path>, router: SwappableAppRouter) -> Result<()> {
    let (tx, rx) = channel(1);

    let mut debouncer = new_debouncer(MONITOR_FS_INTERVAL, move |res: DebounceEventResult| {
        tx.blocking_send(res).unwrap();
    })?;

    debouncer
        .watcher()
        .watch(path.as_ref(), RecursiveMode::Recursive)?;

    let mut stream = ReceiverStream::new(rx);

    while let Some(res) = stream.next().await {
        match res {
            Ok(events) => {
                let mut need_reload = false;
                for event in events {
                    let path = event.path;
                    let ext = path.extension().unwrap_or_default();
                    if path.ends_with("config.yml") || ext == "ts" || ext == "js" {
                        info!("file changed: {}", path.display());
                        need_reload = true;
                        break;
                    }
                }
                if need_reload {
                    let (code, config) = get_code_and_config()?;
                    info!("reload code and config");
                    router.swap(code, config.routes)?;

                    // 更新所有 worker
                    let state = dino_server::AppState::get_current();
                    if let Some(state) = state {
                        state.update_worker("localhost")?;
                        info!("worker updated successfully");
                    }
                }
            }
            Err(e) => {
                warn!("watch error: {}", e);
            }
        }
    }

    Ok(())
}
