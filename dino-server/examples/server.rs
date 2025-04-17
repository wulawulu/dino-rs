use anyhow::Result;
use dino_server::{ProjectConfig, SwappableAppRouter, TenantRouter, start_server};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{Layer as _, fmt::Layer, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    let layer = Layer::new().with_filter(LevelFilter::INFO);
    tracing_subscriber::registry().with(layer).init();

    let config: ProjectConfig = ProjectConfig::load("./fixtures/config.yml")?;

    let code = r#"
    (function(){
        async function hello(req){
            return {
                status:201,
                headers:{
                    "content-type":"application/json"
                },
                body: JSON.stringify(req),
            };
        }
        return{hello:hello};
    })();
    "#;
    let tenant_routers = vec![TenantRouter::new(
        "localhost".to_string(),
        SwappableAppRouter::try_new(code, config.routes)?,
    )];
    start_server(8888, tenant_routers).await?;

    Ok(())
}
