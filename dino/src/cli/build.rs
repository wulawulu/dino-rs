use clap::Parser;

use crate::{CmdExecutor, utils::build_project};

#[derive(Debug, Parser)]
pub struct BuildOpts {}

impl CmdExecutor for BuildOpts {
    async fn execute(self) -> anyhow::Result<()> {
        let cur_dir = std::env::current_dir()?.display().to_string();
        let filename = build_project(&cur_dir)?;
        println!("Build success: {}", filename);
        Ok(())
    }
}
