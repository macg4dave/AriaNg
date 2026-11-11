use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tower::ServiceExt;

struct Fixture {
    gateway: Arc<Gateway>,
    calls: Arc<AtomicUsize>,
    failure: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
    token: String,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn credential(label: &str) -> String {
    // Synthetic credentials derived at runtime, never usable outside these fixtures.
    format!("{:x}", Sha256::digest(format!("synthetic-{label}")))
}

async fn fixture() -> Fixture {
    let calls = Arc::new(AtomicUsize::new(0));
    let failure = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let fail = failure.clone();
    let upstream_secret = credential("upstream");
    let expected = upstream_secret.clone();
    let app = Router::new().route("/rpc", post(move |axum::Json(body): axum::Json<Value>| {
        let count = count.clone();
        let fail = fail.clone();
        let expected = expected.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            if body["method"].as_str().unwrap_or("").starts_with("aria2.") {
                assert!(body["params"][0] == format!("token:{expected}"), "upstream credential injection failed");
            }
            match fail.load(Ordering::SeqCst) {
                1 => return (StatusCode::OK, axum::Json(json!({"jsonrpc":"2.0", "id":"gateway", "error":{"code":1,"message":expected}}))).into_response(),
                2 => return (StatusCode::OK, "x".repeat(MAX_BODY + 1)).into_response(),
                3 => return (StatusCode::TEMPORARY_REDIRECT, [(header::LOCATION, "/redirect")]).into_response(),
                4 => tokio::time::sleep(Duration::from_secs(30)).await,
                _ => {},
            }
            axum::Json(json!({"jsonrpc":"2.0", "id":"gateway", "result":{"method":body["method"], "value":count.load(Ordering::SeqCst)}})).into_response()
        }
    }));
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let token = credential("browser");
    let config = Config {
        listen: (std::net::Ipv4Addr::LOCALHOST, 0).into(),
        browser_origin: "https://example.invalid".into(),
        servers: vec![ServerConfig {
            id: "fixture".into(),
            upstream: format!("http://{addr}/rpc"),
            upstream_secret,
            browser_token: token.clone(),
        }],
    };
    Fixture {
        gateway: Gateway::new(config).unwrap(),
        calls,
        failure,
        task,
        token,
    }
}

async fn request(f: &Fixture, server: &str, token: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/v1/servers/{server}/rpc"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = f.gateway.router().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), MAX_BODY).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn call(method: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":"client", "method":method, "params":[]})
}

#[tokio::test]
async fn credentials_and_origin_are_checked_before_upstream_access() {
    let f = fixture().await;
    assert_eq!(
        request(&f, "fixture", "wrong", call("aria2.getVersion"))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&f, "other", &f.token, call("aria2.getVersion"))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let request = Request::builder()
        .method("POST")
        .uri("/v1/servers/fixture/rpc")
        .header(header::AUTHORIZATION, format!("Bearer {}", f.token))
        .header(header::ORIGIN, "https://other.invalid")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        f.gateway.router().oneshot(request).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrent_clients_share_one_snapshot_and_keep_their_ids() {
    let f = fixture().await;
    let start = Instant::now();
    let mut clients = Vec::new();
    for i in 0..20 {
        let gateway = f.gateway.clone();
        let token = f.token.clone();
        clients.push(tokio::spawn(async move {
            let request = Request::builder()
                .method("POST")
                .uri("/v1/servers/fixture/rpc")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"jsonrpc":"2.0", "id":i, "method":"aria2.getGlobalStat"}).to_string(),
                ))
                .unwrap();
            let response = gateway.router().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), MAX_BODY).await.unwrap())
                    .unwrap();
            assert_eq!(body["id"], i);
        }));
    }
    for client in clients {
        client.await.unwrap();
    }
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    eprintln!(
        "20 simultaneous snapshot clients: 1 upstream request; elapsed {:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn polls_refresh_and_fail_closed_then_recover() {
    let f = fixture().await;
    assert_eq!(
        request(&f, "fixture", &f.token, call("aria2.tellActive"))
            .await
            .0,
        StatusCode::OK
    );
    let scope = &f.gateway.scopes["fixture"];
    for slot in scope.slots.lock().await.values_mut() {
        slot.updated -= Duration::from_secs(2);
    }
    f.failure.store(1, Ordering::SeqCst);
    f.gateway.poll_scope(scope).await;
    let (status, body) = request(&f, "fixture", &f.token, call("aria2.tellActive")).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"]["message"], "RPC request failed");
    assert!(!body.to_string().contains(&credential("upstream")));
    f.failure.store(0, Ordering::SeqCst);
    for slot in scope.slots.lock().await.values_mut() {
        slot.updated -= Duration::from_secs(2);
    }
    f.gateway.poll_scope(scope).await;
    assert_eq!(
        request(&f, "fixture", &f.token, call("aria2.tellActive"))
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn mutation_invalidates_snapshots_and_is_never_replayed() {
    let f = fixture().await;
    request(&f, "fixture", &f.token, call("aria2.getGlobalStat")).await;
    request(&f, "fixture", &f.token, call("aria2.pauseAll")).await;
    request(&f, "fixture", &f.token, call("aria2.getGlobalStat")).await;
    assert_eq!(f.calls.load(Ordering::SeqCst), 3);
    f.gateway.poll_scope(&f.gateway.scopes["fixture"]).await;
    assert_eq!(f.calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn multicall_keeps_aria2_wrapping_and_rejects_nested_batches_before_mutations() {
    let f = fixture().await;
    let batch = json!({"jsonrpc":"2.0","id":7,"method":"system.multicall","params":[[
        {"methodName":"aria2.pauseAll","params":[]},
        {"methodName":"system.multicall","params":[]}
    ]]});
    assert_eq!(
        request(&f, "fixture", &f.token, batch).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    let batch = json!({"jsonrpc":"2.0","id":7,"method":"system.multicall","params":[[
        {"methodName":"aria2.getGlobalStat","params":[]},
        {"methodName":"aria2.getVersion","params":[]}
    ]]});
    let (status, body) = request(&f, "fixture", &f.token, batch).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], 7);
    assert_eq!(body["result"][0][0]["method"], "aria2.getGlobalStat");
    assert_eq!(body["result"][1][0]["method"], "aria2.getVersion");
}

#[tokio::test]
async fn oversized_responses_redirects_and_overload_are_rejected_then_recover() {
    let f = fixture().await;
    for failure in [2, 3] {
        f.failure.store(failure, Ordering::SeqCst);
        assert_eq!(
            request(&f, "fixture", &f.token, call("aria2.getVersion"))
                .await
                .0,
            StatusCode::BAD_GATEWAY
        );
    }
    let permits = f.gateway.admission.acquire_many(32).await.unwrap();
    assert_eq!(
        request(&f, "fixture", &f.token, call("aria2.getVersion"))
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    drop(permits);
    f.failure.store(0, Ordering::SeqCst);
    assert_eq!(
        request(&f, "fixture", &f.token, call("aria2.getVersion"))
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn snapshot_bound_and_idle_eviction() {
    let f = fixture().await;
    for i in 0..MAX_SLOTS {
        let mut body = call("aria2.tellWaiting");
        body["params"] = json!([i, 1]);
        assert_eq!(
            request(&f, "fixture", &f.token, body).await.0,
            StatusCode::OK
        );
    }
    assert_eq!(
        request(&f, "fixture", &f.token, call("aria2.getGlobalStat"))
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let scope = &f.gateway.scopes["fixture"];
    for slot in scope.slots.lock().await.values_mut() {
        slot.accessed -= IDLE;
    }
    f.gateway.poll_scope(scope).await;
    assert!(scope.slots.lock().await.is_empty());
    assert_eq!(
        request(&f, "fixture", &f.token, call("aria2.getGlobalStat"))
            .await
            .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn polling_shutdown_cancels_slow_upstream() {
    let f = fixture().await;
    request(&f, "fixture", &f.token, call("aria2.getGlobalStat")).await;
    for slot in f.gateway.scopes["fixture"].slots.lock().await.values_mut() {
        slot.updated -= FRESH;
    }
    f.failure.store(4, Ordering::SeqCst);
    let (stop, receiver) = watch::channel(false);
    let tasks = f.gateway.start_polling(receiver);
    tokio::time::sleep(Duration::from_millis(30)).await;
    stop.send(true).unwrap();
    for task in tasks {
        timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
    }
    assert!(f.gateway.scopes["fixture"].gate.try_lock().is_ok());
}

#[tokio::test]
async fn browser_cannot_supply_an_upstream_secret_or_arbitrary_method() {
    let f = fixture().await;
    let mut body = call("aria2.getVersion");
    body["params"] = json!([format!("token:{}", credential("injected"))]);
    assert_eq!(
        request(&f, "fixture", &f.token, body).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&f, "fixture", &f.token, call("custom.method"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn scope_credentials_and_snapshots_never_cross_server_boundaries() {
    let mut f = fixture().await;
    let original = &f.gateway.scopes["fixture"].config;
    let upstream = original.upstream.clone();
    let secret = original.upstream_secret.clone();
    let second_token = credential("second-browser");
    f.gateway = Gateway::new(Config {
        listen: (std::net::Ipv4Addr::LOCALHOST, 0).into(),
        browser_origin: "https://example.invalid".into(),
        servers: vec![
            ServerConfig {
                id: "fixture".into(),
                upstream: upstream.clone(),
                upstream_secret: secret.clone(),
                browser_token: f.token.clone(),
            },
            ServerConfig {
                id: "second".into(),
                upstream,
                upstream_secret: secret,
                browser_token: second_token.clone(),
            },
        ],
    })
    .unwrap();
    assert_eq!(
        request(&f, "second", &f.token, call("aria2.getGlobalStat"))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&f, "fixture", &second_token, call("aria2.getGlobalStat"))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&f, "fixture", &f.token, call("aria2.getGlobalStat"))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        request(&f, "second", &second_token, call("aria2.getGlobalStat"))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn body_limits_and_malformed_batches_do_not_execute_mutations() {
    let f = fixture().await;
    let request_body = "x".repeat(MAX_BODY + 1);
    let incoming = Request::builder()
        .method("POST")
        .uri("/v1/servers/fixture/rpc")
        .header(header::AUTHORIZATION, format!("Bearer {}", f.token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(request_body))
        .unwrap();
    assert_eq!(
        f.gateway.router().oneshot(incoming).await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
    let batch = json!({"jsonrpc":"2.0","id":12,"method":"system.multicall","params":[[
        {"methodName":"aria2.pauseAll", "params":[]},
        {"methodName":"aria2.getVersion", "params":42}
    ]]});
    let (status, response) = request(&f, "fixture", &f.token, batch).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(response["id"], 12);
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
}
