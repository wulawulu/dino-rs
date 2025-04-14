mod bundle;

pub use bundle::run_bundle;

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn bundle_ts_should_work() -> Result<()> {
        let ret = run_bundle("fixtures/main.ts", &Default::default())?;
        assert_eq!(
            ret,
            "(function(){async function execute(name){console.log(\"Executing lib\");return`Hello ${name}!`;}async function main(){console.log(\"Executing main\");console.log(await execute(\"world\"));}return{default:main};})();"
        );
        Ok(())
    }
}
