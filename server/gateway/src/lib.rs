use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Path, Request, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};
use subtle::ConstantTimeEq;
use tokio::{
    sync::{Mutex, Semaphore, watch},
    time::{Instant, timeout},
};

mod tasks;

const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_SLOTS: usize = 8;
const FRESH: Duration = Duration::from_secs(1);
const IDLE: Duration = Duration::from_secs(30);
const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub listen: SocketAddr,
    #[serde(default)]
    pub allow_network_listener: bool,
    pub browser_origin: String,
    pub servers: Vec<ServerConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub id: String,
    pub upstream: String,
    pub upstream_secret: String,
    pub browser_token: String,
}

struct Scope {
    config: ServerConfig,
    token_hash: [u8; 32],
    // One gate serializes polling, detail calls and mutations for this scope.
    // Only admitted HTTP requests may queue on it (32 across the process).
    gate: Mutex<()>,
    slots: Mutex<HashMap<String, Slot>>,
}

struct Slot {
    method: String,
    params: Vec<Value>,
    accessed: Instant,
    updated: Instant,
    result: Result<Arc<Value>, Fault>,
}

#[derive(Clone, Copy, Debug)]
struct Fault(StatusCode, i32, &'static str);

const INVALID: Fault = Fault(StatusCode::BAD_REQUEST, -32600, "Invalid Request");
const UNAVAILABLE: Fault = Fault(StatusCode::BAD_GATEWAY, -32000, "Upstream unavailable");
const BUSY: Fault = Fault(StatusCode::SERVICE_UNAVAILABLE, -32001, "Gateway busy");
const DENIED: Fault = Fault(StatusCode::UNAUTHORIZED, 1, "Unauthorized");

impl Fault {
    fn value(self) -> Value {
        json!({"code": self.1, "message": self.2})
    }
    fn response(self, id: Value) -> Response {
        let mut response = (
            self.0,
            axum::Json(json!({"jsonrpc":"2.0", "id":id, "error":self.value()})),
        )
            .into_response();
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
    }
}

pub struct Gateway {
    origin: String,
    scopes: HashMap<String, Arc<Scope>>,
    client: reqwest::Client,
    admission: Semaphore,
}

impl Gateway {
    pub fn new(config: Config) -> Result<Arc<Self>, &'static str> {
        if (!config.listen.ip().is_loopback() && !config.allow_network_listener)
            || config.servers.is_empty()
            || config.servers.len() > 16
        {
            return Err("invalid listener or server count");
        }
        let origin = reqwest::Url::parse(&config.browser_origin).map_err(|_| "invalid origin")?;
        if !matches!(origin.scheme(), "http" | "https")
            || origin.origin().ascii_serialization() != config.browser_origin
        {
            return Err("invalid origin");
        }
        let mut scopes = HashMap::new();
        let mut hashes = Vec::new();
        for server in config.servers {
            let url = reqwest::Url::parse(&server.upstream).map_err(|_| "invalid upstream")?;
            if server.id.is_empty()
                || server.id.len() > 64
                || !server
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || server.browser_token.len() < 32
                || server.browser_token.len() > 256
                || !server.browser_token.bytes().all(|b| b.is_ascii_graphic())
                || server.upstream_secret.len() > 1024
                || server.upstream_secret.is_empty()
            {
                return Err("invalid server configuration");
            }
            let token_hash: [u8; 32] = Sha256::digest(server.browser_token.as_bytes()).into();
            // Separate scopes must have separate browser credentials even for the same URL.
            if hashes.contains(&token_hash) {
                return Err("duplicate credential");
            }
            hashes.push(token_hash);
            let id = server.id.clone();
            if scopes
                .insert(
                    id,
                    Arc::new(Scope {
                        config: server,
                        token_hash,
                        gate: Mutex::new(()),
                        slots: Mutex::new(HashMap::new()),
                    }),
                )
                .is_some()
            {
                return Err("duplicate server");
            }
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(3))
            .timeout(DEADLINE)
            .pool_max_idle_per_host(1)
            .build()
            .map_err(|_| "client initialization failed")?;
        Ok(Arc::new(Self {
            origin: config.browser_origin,
            scopes,
            client,
            admission: Semaphore::new(32),
        }))
    }

    pub fn router(self: &Arc<Self>) -> Router {
        Router::new()
            .route("/healthz", get(|| async { "ariang-gateway/v1" }))
            .route("/v1/servers/{server}/rpc", post(rpc))
            .with_state(self.clone())
    }

    pub fn start_polling(
        self: &Arc<Self>,
        stop: watch::Receiver<bool>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        self.scopes
            .values()
            .map(|scope| {
                let gateway = self.clone();
                let scope = scope.clone();
                let mut stop = stop.clone();
                tokio::spawn(async move {
                    let mut timer = tokio::time::interval(FRESH);
                    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        tokio::select! {
                            _ = stop.changed() => break,
                            _ = timer.tick() => {
                                tokio::select! {
                                    _ = stop.changed() => break,
                                    _ = gateway.poll_scope(&scope) => {},
                                }
                            }
                        }
                    }
                })
            })
            .collect()
    }

    async fn poll_scope(&self, scope: &Scope) {
        let requests = {
            let mut slots = scope.slots.lock().await;
            slots.retain(|_, slot| slot.accessed.elapsed() < IDLE);
            slots
                .iter()
                .filter(|(_, slot)| slot.updated.elapsed() >= FRESH)
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>()
        };
        for key in requests {
            // Release between slots so a slow poll cannot monopolize all eight
            // deadlines ahead of task controls. Recheck after acquiring the gate:
            // a mutation or client may have invalidated/refreshed this slot.
            let _gate = scope.gate.lock().await;
            let request = scope.slots.lock().await.get(&key).and_then(|slot| {
                (slot.accessed.elapsed() < IDLE && slot.updated.elapsed() >= FRESH)
                    .then(|| (slot.method.clone(), slot.params.clone()))
            });
            let Some((method, params)) = request else {
                continue;
            };
            let result = self.upstream(scope, &method, params).await.map(Arc::new);
            if let Some(slot) = scope.slots.lock().await.get_mut(&key) {
                slot.updated = Instant::now();
                slot.result = result;
            }
        }
    }

    async fn upstream(
        &self,
        scope: &Scope,
        method: &str,
        mut params: Vec<Value>,
    ) -> Result<Value, Fault> {
        let summary_source = tasks::source(method);
        let method = if let Some(source) = summary_source {
            params = tasks::upstream_params(source);
            source
        } else {
            method
        };
        if method.starts_with("aria2.") {
            params.insert(
                0,
                Value::String(format!("token:{}", scope.config.upstream_secret)),
            );
        }
        let mut response = self
            .client
            .post(&scope.config.upstream)
            .json(&json!({"jsonrpc":"2.0", "id":"gateway", "method":method, "params":params}))
            .send()
            .await
            .map_err(|_| UNAVAILABLE)?;
        if !response.status().is_success() {
            return Err(UNAVAILABLE);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| UNAVAILABLE)? {
            if bytes.len() + chunk.len() > MAX_BODY {
                return Err(UNAVAILABLE);
            }
            bytes.extend_from_slice(&chunk);
        }
        let mut value: Value = serde_json::from_slice(&bytes).map_err(|_| UNAVAILABLE)?;
        if value["jsonrpc"] != "2.0" || value["id"] != "gateway" {
            return Err(UNAVAILABLE);
        }
        if value.get("error").is_some() {
            // Do not expose raw upstream messages, paths or credentials.
            return Err(Fault(StatusCode::BAD_GATEWAY, -32002, "RPC request failed"));
        }
        let result = value
            .get_mut("result")
            .map(Value::take)
            .ok_or(UNAVAILABLE)?;
        if summary_source.is_some() {
            tasks::compact(result)
        } else {
            Ok(result)
        }
    }

    async fn call(
        &self,
        scope: &Scope,
        method: &str,
        params: Vec<Value>,
    ) -> Result<(Arc<Value>, u128), Fault> {
        let _gate = scope.gate.lock().await;
        if cacheable(method) {
            let key = serde_json::to_string(&(method, &params)).map_err(|_| INVALID)?;
            if key.len() > 4096 {
                return Err(INVALID);
            }
            {
                let mut slots = scope.slots.lock().await;
                slots.retain(|_, slot| slot.accessed.elapsed() < IDLE);
                if let Some(slot) = slots.get_mut(&key) {
                    slot.accessed = Instant::now();
                    if slot.updated.elapsed() < FRESH {
                        return slot
                            .result
                            .clone()
                            .map(|v| (v, slot.updated.elapsed().as_millis()));
                    }
                } else if slots.len() >= MAX_SLOTS {
                    return Err(BUSY);
                }
            }
            let result = self
                .upstream(scope, method, params.clone())
                .await
                .map(Arc::new);
            scope.slots.lock().await.insert(
                key,
                Slot {
                    method: method.into(),
                    params,
                    accessed: Instant::now(),
                    updated: Instant::now(),
                    result: result.clone(),
                },
            );
            result.map(|v| (v, 0))
        } else {
            // Invalidate before sending: a timeout may still have applied the mutation.
            if !read_only(method) {
                scope.slots.lock().await.clear();
            }
            self.upstream(scope, method, params)
                .await
                .map(|v| (Arc::new(v), 0))
        }
    }
}

fn cacheable(method: &str) -> bool {
    tasks::source(method).is_some()
        || matches!(
            method,
            "aria2.tellActive" | "aria2.tellWaiting" | "aria2.tellStopped" | "aria2.getGlobalStat"
        )
}

fn read_only(method: &str) -> bool {
    (cacheable(method) && tasks::source(method).is_none())
        || method == "gateway.listTasks"
        || matches!(
            method,
            "aria2.tellStatus"
                | "aria2.getUris"
                | "aria2.getFiles"
                | "aria2.getPeers"
                | "aria2.getServers"
                | "aria2.getOption"
                | "aria2.getGlobalOption"
                | "aria2.getVersion"
                | "aria2.getSessionInfo"
                | "system.listMethods"
                | "system.listNotifications"
        )
}

fn allowed(method: &str) -> bool {
    read_only(method)
        || matches!(
            method,
            "aria2.addUri"
                | "aria2.addTorrent"
                | "aria2.addMetalink"
                | "aria2.remove"
                | "aria2.forceRemove"
                | "aria2.pause"
                | "aria2.pauseAll"
                | "aria2.forcePause"
                | "aria2.forcePauseAll"
                | "aria2.unpause"
                | "aria2.unpauseAll"
                | "aria2.changePosition"
                | "aria2.changeUri"
                | "aria2.changeOption"
                | "aria2.changeGlobalOption"
                | "aria2.purgeDownloadResult"
                | "aria2.removeDownloadResult"
                | "aria2.shutdown"
                | "aria2.forceShutdown"
                | "aria2.saveSession"
        )
}

fn validate_call(method: &str, params: &[Value]) -> Result<(), Fault> {
    if !allowed(method)
        || params.len() > 16
        || params
            .first()
            .and_then(Value::as_str)
            .is_some_and(|s| s.starts_with("token:"))
    {
        return Err(INVALID);
    }
    Ok(())
}

async fn rpc(
    State(gateway): State<Arc<Gateway>>,
    Path(server): Path<String>,
    request: Request,
) -> Response {
    let Some(scope) = gateway.scopes.get(&server) else {
        return DENIED.response(Value::Null);
    };
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    if !bool::from(digest.ct_eq(&scope.token_hash)) {
        return DENIED.response(Value::Null);
    }
    if request
        .headers()
        .get(header::ORIGIN)
        .is_some_and(|v| v.as_bytes() != gateway.origin.as_bytes())
    {
        return DENIED.response(Value::Null);
    }
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_none_or(|v| v.split(';').next().unwrap_or("").trim() != "application/json")
    {
        return INVALID.response(Value::Null);
    }
    let Ok(_permit) = gateway.admission.try_acquire() else {
        return BUSY.response(Value::Null);
    };
    let mut response_id = Value::Null;
    let operation = async {
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| INVALID)?;
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
        let id = body.get("id").cloned().ok_or(INVALID)?;
        if body["jsonrpc"] != "2.0"
            || !(id.is_string() || id.is_number() || id.is_null())
            || id.as_str().is_some_and(|s| s.len() > 128)
        {
            return Err(INVALID);
        }
        response_id = id.clone();
        let method = body["method"].as_str().ok_or(INVALID)?;
        let params = match body.get("params") {
            Some(Value::Array(v)) => v.clone(),
            None => Vec::new(),
            _ => return Err(INVALID),
        };
        let result = if method == "system.multicall" {
            if params.len() != 1 {
                return Err(INVALID);
            }
            let calls = params[0].as_array().ok_or(INVALID)?;
            if calls.is_empty() || calls.len() > 64 {
                return Err(INVALID);
            }
            // Validate the entire batch before executing any mutations.
            let mut parsed = Vec::new();
            for call in calls {
                let method = call["methodName"].as_str().ok_or(INVALID)?;
                let params = match call.get("params") {
                    Some(Value::Array(v)) => v.clone(),
                    None => Vec::new(),
                    _ => return Err(INVALID),
                };
                validate_call(method, &params)?;
                parsed.push((method, params));
            }
            let mut results = Vec::new();
            let mut length = 0;
            let mut age = 0;
            for (method, params) in parsed {
                let result = match gateway.dispatch(scope, method, params).await {
                    Ok((value, snapshot_age)) => {
                        age = age.max(snapshot_age);
                        json!([value])
                    }
                    Err(error) => error.value(),
                };
                length += serde_json::to_vec(&result).map_err(|_| UNAVAILABLE)?.len() + 1;
                if length > MAX_BODY - 1024 {
                    return Err(UNAVAILABLE);
                }
                results.push(result);
            }
            (Arc::new(Value::Array(results)), age)
        } else {
            validate_call(method, &params)?;
            match gateway.dispatch(scope, method, params).await {
                Ok(result) => result,
                Err(error) => return Ok(error.response(id)),
            }
        };
        #[derive(Serialize)]
        struct RpcResponse {
            jsonrpc: &'static str,
            id: Value,
            result: Arc<Value>,
        }
        let bytes = serde_json::to_vec(&RpcResponse {
            jsonrpc: "2.0",
            id,
            result: result.0,
        })
        .map_err(|_| UNAVAILABLE)?;
        if bytes.len() > MAX_BODY {
            return Err(UNAVAILABLE);
        }
        let mut response = Response::new(Body::from(bytes));
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        response.headers_mut().insert(
            "x-snapshot-age-ms",
            HeaderValue::from_str(&result.1.to_string()).map_err(|_| INVALID)?,
        );
        Ok(response)
    };
    let mut response = match timeout(Duration::from_secs(15), operation).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => error.response(response_id),
        Err(_) => UNAVAILABLE.response(response_id),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[derive(Debug)]
pub struct RunError;

pub async fn run(path: &str) -> Result<(), RunError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| RunError)?;
    let mut text = String::new();
    file.take(64 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|_| RunError)?;
    if text.len() > 64 * 1024 {
        return Err(RunError);
    }
    let config: Config = toml::from_str(&text).map_err(|_| RunError)?;
    let listen = config.listen;
    let gateway = Gateway::new(config).map_err(|_| RunError)?;
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|_| RunError)?;
    let (stop, receiver) = watch::channel(false);
    let polling = gateway.start_polling(receiver);
    let shutdown = async move {
        #[cfg(unix)]
        {
            if let Ok(mut term) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
            } else {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        let _ = stop.send(true);
    };
    let result = axum::serve(listener, gateway.router())
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|_| RunError);
    for task in polling {
        task.abort();
        let _ = task.await;
    }
    result
}

#[cfg(test)]
mod tests;
