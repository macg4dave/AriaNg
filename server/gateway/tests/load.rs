//! Explicit synthetic comparison; never contacts private configuration or services.
use ariang_gateway::{Config, Gateway, ServerConfig};
use axum::{Router, extract::Json, routing::post};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    net::Ipv4Addr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{sync::watch, task::JoinSet};

#[cfg(windows)]
fn resources() -> (f64, u64) {
    // Ask the OS for this test process only. No request/config values enter commands.
    let command = format!(
        "$p = Get-Process -Id {}; [Console]::Write(('{{0}}|{{1}}' -f $p.TotalProcessorTime.TotalMilliseconds, $p.WorkingSet64))",
        std::process::id()
    );
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let (cpu, rss) = text.trim().split_once('|').unwrap();
    (cpu.parse().unwrap(), rss.parse().unwrap())
}

#[cfg(target_os = "linux")]
fn resources() -> (f64, u64) {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    let ticks: f64 = fields[11].parse::<f64>().unwrap() + fields[12].parse::<f64>().unwrap();
    let hz = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .unwrap();
    let hz: f64 = String::from_utf8(hz.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let rss = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap()
        * 1024;
    (ticks * 1000.0 / hz, rss)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn resources() -> (f64, u64) {
    (0.0, 0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit release-mode synthetic load comparison, about ten seconds"]
async fn compare_direct_and_shared_polling() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let tasks: Vec<_> = (0..25).map(|i| json!({
        "gid":format!("{i:016x}"), "status":"active", "totalLength":"1024", "completedLength":"512",
        "files":(0..100).map(|j| json!({"index":j+1,"path":format!("/synthetic/task-{i}/file-{j}"),"length":"1024","completedLength":"512","selected":"true","uris":[]})).collect::<Vec<_>>()
    })).collect();
    let payload = Arc::new(Value::Array(tasks));
    let app = Router::new().route(
        "/rpc",
        post(move |Json(body): Json<Value>| {
            counter.fetch_add(1, Ordering::SeqCst);
            let result = payload.clone();
            async move { Json(json!({"jsonrpc":"2.0","id":body["id"],"result":*result})) }
        }),
    );
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let upstream = format!("http://{}/rpc", listener.local_addr().unwrap());
    let serving = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let token = format!("{:x}", Sha256::digest("synthetic-load-browser"));
    let secret = format!("{:x}", Sha256::digest("synthetic-load-upstream"));
    let gateway = Gateway::new(Config {
        listen: (Ipv4Addr::LOCALHOST, 0).into(),
        browser_origin: "https://example.invalid".into(),
        servers: vec![ServerConfig {
            id: "load".into(),
            upstream: upstream.clone(),
            upstream_secret: secret.clone(),
            browser_token: token.clone(),
        }],
    })
    .unwrap();
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let endpoint = format!(
        "http://{}/v1/servers/load/rpc",
        listener.local_addr().unwrap()
    );
    let router = gateway.router();
    let api = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let (stop, receiver) = watch::channel(false);
    let polling = gateway.start_polling(receiver);
    let client = reqwest::Client::new();
    for mode in ["direct", "gateway"] {
        let before_calls = calls.load(Ordering::SeqCst);
        let (cpu_before, _) = resources();
        let start = Instant::now();
        let mut latencies = Vec::new();
        let mut errors = 0;
        let mut bytes = 0;
        for round in 0..5 {
            tokio::time::sleep_until(tokio::time::Instant::from_std(
                start + Duration::from_secs(round),
            ))
            .await;
            let mut clients = JoinSet::new();
            for _ in 0..20 {
                let client = client.clone();
                let url = if mode == "direct" {
                    upstream.clone()
                } else {
                    endpoint.clone()
                };
                let params = if mode == "direct" {
                    json!([format!("token:{secret}")])
                } else {
                    json!([])
                };
                let token = token.clone();
                clients.spawn(async move {
                    let start = Instant::now();
                    let response = client.post(url).bearer_auth(token)
                        .json(&json!({"jsonrpc":"2.0","id":"gateway","method":"aria2.tellActive","params":params}))
                        .send().await.unwrap();
                    let ok = response.status().is_success();
                    let body = response.bytes().await.unwrap();
                    (start.elapsed().as_secs_f64() * 1000.0, ok, body.len())
                });
            }
            while let Some(result) = clients.join_next().await {
                let (latency, ok, length) = result.unwrap();
                latencies.push(latency);
                errors += usize::from(!ok);
                bytes += length;
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        let upstream_calls = calls.load(Ordering::SeqCst) - before_calls;
        let (cpu_after, rss) = resources();
        latencies.sort_by(f64::total_cmp);
        println!(
            "{}",
            json!({"mode":mode,"clients":20,"tasks":25,"files_per_task":100,"rounds":5,
            "requests":latencies.len(),"upstream_requests":upstream_calls,"errors":errors,"response_bytes":bytes,
            "elapsed_s":elapsed,"requests_per_s":latencies.len() as f64 / elapsed,
            "p95_ms":latencies[94],"p99_ms":latencies[98],"combined_process_cpu_ms":cpu_after-cpu_before,"process_rss_bytes":rss})
        );
        assert_eq!(errors, 0);
        if mode == "direct" {
            assert_eq!(upstream_calls, 100);
        } else {
            assert!(upstream_calls < 20);
        }
    }
    stop.send(true).unwrap();
    for task in polling {
        task.await.unwrap();
    }
    serving.abort();
    api.abort();
}
