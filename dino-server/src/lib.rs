use std::collections::HashMap;

use anyhow::Result;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Query, State},
    http::{Method, Uri},
    response::IntoResponse,
    routing::any,
};
use axum_extra::extract::Host;
use dashmap::DashMap;
use error::AppError;
use serde_json::json;
use tokio::net::TcpListener;
use tracing::info;

mod config;
pub mod engine;
mod error;
mod router;

pub use config::ProjectConfig;
pub use router::SwappableAppRouter;

#[derive(Clone)]
pub struct AppState {
    routers: DashMap<String, SwappableAppRouter>,
}

pub async fn start_server(port: u16, routers: DashMap<String, SwappableAppRouter>) -> Result<()> {
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(addr).await?;

    info!("Listening on: {}", listener.local_addr()?);
    let state = AppState::new(routers);
    let app = Router::new()
        .route("/{*path}", any(handler))
        .with_state(state);
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}

#[axum::debug_handler]
async fn handler(
    State(state): State<AppState>,
    Query(query): Query<serde_json::Value>,
    method: Method,
    Host(mut host): Host,
    uri: Uri,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let _ = host.split_off(host.find(':').unwrap_or(host.len()));

    let router = state
        .routers
        .get(&host)
        .ok_or(AppError::RoutePathNotFound(uri.to_string()))?
        .load();

    let matched = router.match_it(method, uri.path())?;
    let handler = matched.value;
    let params: HashMap<String, String> = matched
        .params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let body = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body)?
    };

    Ok(Json(json!({
        "params": params,
        "body": body,
        "query": query,
        "handler": handler,
    })))
}

impl AppState {
    pub fn new(routers: DashMap<String, SwappableAppRouter>) -> Self {
        Self { routers }
    }
}
