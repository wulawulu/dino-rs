use std::{collections::HashMap, fs};

use clap::Parser;

use crate::{
    CmdExecutor,
    engine::{JsWorker, Req},
    utils::build_project,
};

#[derive(Debug, Parser)]
pub struct RunOpts {}

impl CmdExecutor for RunOpts {
    async fn execute(self) -> anyhow::Result<()> {
        let filename = build_project(".")?;
        let content = fs::read_to_string(filename)?;
        let worker = JsWorker::try_new(&content)?;
        let req = Req::builder()
            .method("GET")
            .url("https://example.com")
            .headers(HashMap::new())
            .build();
        let ret = worker.run("hello", req)?;
        println!("Response: {:?}", ret);
        Ok(())
    }
}
