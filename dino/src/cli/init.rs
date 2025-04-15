use std::{fs, path::Path};

use askama::Template;
use clap::Parser;
use dialoguer::Input;
use git2::Repository;

use crate::CmdExecutor;

#[derive(Debug, Parser)]
pub struct InitOpts {}

#[derive(Template)]
#[template(path = "config.yml.j2")]
struct ConfigFile {
    name: String,
}

#[derive(Template)]
#[template(path = "main.ts.j2")]
struct MainFile {}

#[derive(Template)]
#[template(path = ".gitignore.j2")]
struct GitignoreFile {}

impl CmdExecutor for InitOpts {
    async fn execute(self) -> anyhow::Result<()> {
        let name: String = Input::new().with_prompt("Project name").interact_text()?;

        // if current dir is empty then init project, otherwise create new dir and init project
        let cur = Path::new(".");
        if fs::read_dir(cur)?.next().is_none() {
            init_project(&name, cur)?;
        } else {
            let new_dir = cur.join(&name);
            init_project(&name, &new_dir)?;
        }

        Ok(())
    }
}

fn init_project(name: &str, path: &Path) -> anyhow::Result<()> {
    Repository::init(path)?;

    let config = ConfigFile {
        name: name.to_string(),
    };
    fs::write(path.join("config.yml"), config.render()?)?;
    fs::write(path.join("main.ts"), MainFile {}.render()?)?;
    fs::write(path.join(".gitignore"), GitignoreFile {}.render()?)?;

    Ok(())
}
