use ariang_gateway::{Config, Gateway, ServerConfig};
use sha2::{Digest, Sha256};
use std::{net::Ipv4Addr, path::PathBuf, time::Duration};
use tokio::{process::Command, sync::watch, time::timeout};

#[tokio::test]
#[ignore = "requires healthy disposable Compose web/aria2, Node, optional Playwright and Edge"]
async fn browser_and_real_aria2() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Only the repository's disposable Compose fixture credential is read.
    let secret = std::fs::read_to_string(root.join(".local/aria2/rpc-secret"))
        .expect("fixture credential missing");
    let token = format!(
        "{:x}",
        Sha256::digest(format!(
            "{:?}-browser-fixture",
            std::time::SystemTime::now()
        ))
    );
    let base = "http://127.0.0.1:8080";
    let gateway = Gateway::new(Config {
        allow_network_listener: false,
        listen: (Ipv4Addr::LOCALHOST, 0).into(),
        browser_origin: base.into(),
        servers: vec![ServerConfig {
            id: "fixture".into(),
            upstream: format!("{base}/jsonrpc"),
            upstream_secret: secret.trim().into(),
            browser_token: token.clone(),
        }],
    })
    .unwrap();
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let router = gateway.router();
    let serving = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let (stop, receiver) = watch::channel(false);
    let polling = gateway.start_polling(receiver);
    let child = Command::new("node")
        .arg("tests/gateway-browser.cjs")
        .current_dir(root)
        .env(
            "GATEWAY_FIXTURE_URL",
            format!("http://{address}/v1/servers/fixture/rpc"),
        )
        .env("GATEWAY_FIXTURE_TOKEN", token)
        .env("ARIA2_FIXTURE_TOKEN", secret.trim())
        .kill_on_drop(true)
        .output();
    let result = timeout(Duration::from_secs(120), child).await;
    let _ = stop.send(true);
    for task in polling {
        task.await.unwrap();
    }
    serving.abort();
    let output = result
        .expect("browser check deadline exceeded")
        .expect("Node unavailable");
    // Browser runner emits only sanitized aggregate output.
    assert!(
        output.status.success(),
        "gateway browser/aria2 compatibility check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
