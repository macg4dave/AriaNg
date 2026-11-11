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

#[test]
fn network_listener_requires_explicit_private_configuration() {
    let mut config: Config = serde_json::from_value(json!({
        "listen":"0.0.0.0:8081", "browser_origin":"https://example.invalid",
        "servers":[{"id":"fixture", "upstream":"http://example.invalid/rpc",
            "upstream_secret":credential("upstream"), "browser_token":credential("browser")}]
    }))
    .unwrap();
    assert!(!config.allow_network_listener);
    assert!(Gateway::new(config).is_err());
    config = Config {
        listen: "0.0.0.0:8081".parse().unwrap(),
        allow_network_listener: true,
        browser_origin: "https://example.invalid".into(),
        servers: vec![ServerConfig {
            id: "fixture".into(),
            upstream: "http://example.invalid/rpc".into(),
            upstream_secret: credential("upstream"),
            browser_token: credential("browser"),
        }],
    };
    assert!(Gateway::new(config).is_ok());
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
            let result = if body["params"].as_array().and_then(|p| p.last()).and_then(Value::as_array)
                .is_some_and(|keys| keys.iter().any(|k| k == "infoHash")) {
                json!((0..25).map(|i| json!({
                    "gid":format!("{i:016x}"),"status":"active","totalLength":format!("{}", 1000+i),
                    "completedLength":count.load(Ordering::SeqCst).to_string(),"downloadSpeed":"10","uploadSpeed":"0",
                    "files":(0..100).map(|j| json!({"path":format!("/synthetic/task-{i}/file-{j}"),"selected":"true"})).collect::<Vec<_>>()
                })).collect::<Vec<_>>())
            } else { json!({"method":body["method"], "value":count.load(Ordering::SeqCst)}) };
            axum::Json(json!({"jsonrpc":"2.0", "id":"gateway", "result":result})).into_response()
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
        allow_network_listener: false,
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

fn query(params: Value) -> Value {
    json!({"jsonrpc":"2.0", "id":"query", "method":"gateway.listTasks", "params":[params]})
}

#[tokio::test]
async fn compact_pages_filter_and_sort_before_pagination_without_files() {
    let f = fixture().await;
    let (status, response) = request(
        &f,
        "fixture",
        &f.token,
        query(json!({"source":"active","sort":"size","reverse":true,"offset":2,"limit":3})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let page = &response["result"];
    assert_eq!(page["total"], 25);
    assert_eq!(page["tasks"].as_array().unwrap().len(), 3);
    assert_eq!(page["tasks"][0]["position"], 22);
    assert_eq!(page["tasks"][0]["fileCount"], 100);
    assert_eq!(page["tasks"][0]["selectedFileCount"], 100);
    assert_eq!(page["tasks"][0]["taskName"], "file-0");
    assert!(page["tasks"][0].get("files").is_none());
    let (_, searched) = request(
        &f,
        "fixture",
        &f.token,
        query(json!({"search":"FILE-0","offset":24,"limit":2})),
    )
    .await;
    assert_eq!(searched["result"]["total"], 25);
    assert_eq!(searched["result"]["tasks"][0]["position"], 24);
    assert_eq!(searched["result"]["hasMore"], false);
    let (_, empty) = request(&f, "fixture", &f.token, query(json!({"search":"missing"}))).await;
    assert_eq!(empty["result"]["total"], 0);
    assert_eq!(
        f.calls.load(Ordering::SeqCst),
        1,
        "query parameters must share the same upstream snapshot"
    );
}

#[tokio::test]
async fn pagination_revision_rejects_changed_snapshots_after_mutations() {
    let f = fixture().await;
    let (_, first) = request(&f, "fixture", &f.token, query(json!({"limit":5}))).await;
    let revision = first["result"]["snapshot"].as_str().unwrap();
    let (status, next) = request(
        &f,
        "fixture",
        &f.token,
        query(json!({"offset":5,"limit":5,"snapshot":revision})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(next["result"]["tasks"][0]["position"], 5);
    request(&f, "fixture", &f.token, call("aria2.pauseAll")).await;
    let (status, response) = request(
        &f,
        "fixture",
        &f.token,
        query(json!({"offset":5,"snapshot":revision})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(response["error"]["code"], -32003);
    let (status, _) = request(&f, "fixture", &f.token, query(json!({}))).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn task_query_limits_and_internal_methods_are_not_bypassable() {
    let f = fixture().await;
    for params in [
        json!({"limit":201}),
        json!({"limit":0}),
        json!({"offset":1001}),
        json!({"search":"x".repeat(257)}),
        json!({"source":"unknown"}),
        json!({"sort":"unknown"}),
        json!({"upstream":"https://example.invalid"}),
    ] {
        assert_eq!(
            request(&f, "fixture", &f.token, query(params)).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        request(&f, "fixture", &f.token, call("gateway.tasks.active"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
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
        allow_network_listener: false,
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
