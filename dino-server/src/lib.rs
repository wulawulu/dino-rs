use std::collections::HashMap;

use anyhow::{Context, Result};
use axum::{
    Router,
    body::Bytes,
    extract::{Query, State},
    http::{Method, Response, Uri},
    response::IntoResponse,
    routing::any,
};
use axum_extra::extract::Host;
use dashmap::DashMap;
use engine::{JsWorker, Req};
use error::AppError;
use matchit::Match;
use router::AppRouter;
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

#[derive(Clone)]
pub struct TenantRouter {
    host: String,
    router: SwappableAppRouter,
}

pub async fn start_server(port: u16, routers: Vec<TenantRouter>) -> Result<()> {
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(addr).await?;
    let map = DashMap::new();

    for router in routers {
        map.insert(router.host, router.router);
    }

    info!("Listening on: {}", listener.local_addr()?);
    let state = AppState::new(map);
    let app = Router::new()
        .route("/{*path}", any(handler))
        .with_state(state);
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}

async fn handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    method: Method,
    Host(host): Host,
    uri: Uri,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let router = get_router(host, &state)?;
    let matched = router.match_it(method.clone(), uri.path())?;
    let req = assemble_req(query, &matched, method, &uri, body)?;
    let worker = JsWorker::try_new(&router.code).unwrap();
    let handler = matched.value;
    let resp = worker.run(handler, req).unwrap();

    Ok(Response::from(resp))
}

fn get_router(mut host: String, state: &AppState) -> Result<AppRouter> {
    let _ = host.split_off(host.find(':').unwrap_or(host.len()));

    let router = state
        .routers
        .get(&host)
        .ok_or(AppError::RoutePathNotFound(host.to_string()))?
        .load();
    Ok(router)
}

fn assemble_req(
    query: HashMap<String, String>,
    matched: &Match<&str>,
    method: Method,
    uri: &Uri,
    body: Bytes,
) -> Result<Req> {
    let params: HashMap<String, String> = matched
        .params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let body = if body.is_empty() {
        None
    } else {
        Some(String::from_utf8(body.to_vec()).context("Failed to convert body to string")?)
    };
    let req = Req::builder()
        .method(method.to_string())
        .url(uri.to_string())
        .headers(HashMap::new())
        .query(query)
        .params(params)
        .body(body)
        .build();
    Ok(req)
}

impl AppState {
    pub fn new(routers: DashMap<String, SwappableAppRouter>) -> Self {
        Self { routers }
    }
}

impl TenantRouter {
    pub fn new(host: String, router: SwappableAppRouter) -> Self {
        Self { host, router }
    }
}
