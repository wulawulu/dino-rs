use clap::{Parser, command};
use enum_dispatch::enum_dispatch;

pub use self::{build::*, init::*, run::*};

mod build;
mod init;
mod run;

#[derive(Debug, Parser)]
#[command(name = "dino", version, author, about, long_about = None)]
pub struct Opts {
    #[command(subcommand)]
    pub cmd: SubCommand,
}

#[derive(Debug, Parser)]
#[enum_dispatch(CmdExecutor)]
pub enum SubCommand {
    #[command(name = "init", about = "Initialize a new Dino project")]
    Init(InitOpts),
    #[command(name = "build", about = "Build the project")]
    Build(BuildOpts),
    #[command(name = "run", about = "Run the project")]
    Run(RunOpts),
}
