use std::fs;

use clap::Parser;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{Layer as _, fmt::Layer, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{CmdExecutor, utils::build_project};
use dino_server::{ProjectConfig, SwappableAppRouter, TenantRouter, start_server};
#[derive(Debug, Parser)]
pub struct RunOpts {}

impl CmdExecutor for RunOpts {
    async fn execute(self) -> anyhow::Result<()> {
        let filename = build_project(".")?;
        let config = filename.replace(".mjs", ".yml");
        let code = fs::read_to_string(filename)?;
        let config = ProjectConfig::load(config)?;

        let layer = Layer::new().with_filter(LevelFilter::INFO);
        tracing_subscriber::registry().with(layer).init();

        start_server(
            8888,
            vec![TenantRouter::new(
                "localhost".to_string(),
                SwappableAppRouter::try_new(&code, config.routes)?,
            )],
        )
        .await?;
        Ok(())
    }
}
