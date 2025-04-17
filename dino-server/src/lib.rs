use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    thread,
};

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
use crossbeam::channel::Sender;
use dashmap::DashMap;
use engine::{JsWorker, Req, Resp};
use error::AppError;
use matchit::Match;
use router::AppRouter;
use tokio::net::TcpListener;
use tracing::{error, info};

mod config;
pub mod engine;
mod error;
mod router;

pub use config::ProjectConfig;
pub use router::SwappableAppRouter;

#[derive(Clone, Debug)]
pub struct AppState {
    routers: DashMap<String, SwappableAppRouter>,
    workers: Arc<Mutex<HashMap<String, Sender<WorkerMessage>>>>,
}

#[derive(Clone)]
pub struct TenantRouter {
    host: String,
    router: SwappableAppRouter,
}

static CURRENT_STATE: OnceLock<AppState> = OnceLock::new();

// 添加一个特殊的消息类型用于终止 worker
#[derive(Debug)]
enum WorkerMessage {
    Request(Box<Request>),
    Shutdown,
}

#[derive(Debug)]
struct Request {
    req: Req,
    handler: String,
    send: oneshot::Sender<Resp>,
}

impl WorkerMessage {
    pub fn new_request(req: Req, handler: String) -> (Self, oneshot::Receiver<Resp>) {
        let (send, recv) = oneshot::channel();
        (
            Self::Request(Box::new(Request { req, handler, send })),
            recv,
        )
    }
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
    Host(mut host): Host,
    uri: Uri,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let _ = host.split_off(host.find(':').unwrap_or(host.len()));
    let router = get_router(host.clone(), &state)?;
    let matched = router.match_it(method.clone(), uri.path())?;
    let req = assemble_req(query, &matched, method, &uri, body)?;
    let handler = matched.value;
    let resp = state.send(host, handler.to_string(), req)?;

    Ok(Response::from(resp))
}

fn get_router(host: String, state: &AppState) -> Result<AppRouter> {
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
        let workers = Arc::new(Mutex::new(HashMap::new()));
        for item in &routers {
            let (send, recv) = crossbeam::channel::unbounded::<WorkerMessage>();
            let code = item.value().load().code;
            thread::Builder::new()
                .name(format!("worker-{}", item.key()))
                .spawn(move || jsworker_execute(code, recv))
                .unwrap();
            workers.lock().unwrap().insert(item.key().to_string(), send);
        }
        let state = Self { routers, workers };
        CURRENT_STATE.set(state.clone()).unwrap();
        state
    }

    pub fn get_current() -> Option<&'static AppState> {
        CURRENT_STATE.get()
    }

    pub fn update_worker(&self, host: &str) -> Result<()> {
        let mut workers = self.workers.lock().unwrap();

        // 获取最新的code
        let code = self
            .routers
            .get(host)
            .context("Router not found")?
            .load()
            .code;

        let (new_send, new_recv) = crossbeam::channel::unbounded();
        // 启动新 worker 线程
        thread::Builder::new()
            .name(format!("worker-{}", host))
            .spawn(move || jsworker_execute(code, new_recv))?;

        // 更新 worker 映射
        let old_sender = workers.insert(host.to_string(), new_send);

        // 关闭旧 worker（如果有）
        if let Some(old_sender) = old_sender {
            let _ = old_sender.send(WorkerMessage::Shutdown);
        }

        info!("Worker updated successfully for host: {}", host);
        Ok(())
    }

    pub fn send(&self, host: String, handler: String, req: Req) -> Result<Resp> {
        let workers = self.workers.lock().unwrap();

        let send = workers.get(&host).context("Worker not found")?;
        let (msg, recv) = WorkerMessage::new_request(req, handler);
        if let Err(e) = send.send(msg) {
            error!("Send to jsworker error: {}", e);
        }
        let resp = recv.recv()?;
        Ok(resp)
    }
}

fn jsworker_execute(code: String, recv: crossbeam::channel::Receiver<WorkerMessage>) -> Result<()> {
    let worker = JsWorker::try_new(&code).context("Failed to create worker")?;
    while let Ok(msg) = recv.recv() {
        match msg {
            WorkerMessage::Request(req) => {
                let resp = worker.run(&req.handler, req.req)?;
                if let Err(e) = req.send.send(resp) {
                    error!("Send resp to oneshot error: {}", e);
                }
            }
            WorkerMessage::Shutdown => {
                info!("Worker shutdown");
                return Ok(());
            }
        }
    }
    Ok(())
}

impl TenantRouter {
    pub fn new(host: String, router: SwappableAppRouter) -> Self {
        Self { host, router }
    }
}
