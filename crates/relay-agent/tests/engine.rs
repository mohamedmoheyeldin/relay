#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
//! Engine download, verification and unpacking against a fake release server.

use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use relay_agent::local::engine::{EngineError, EngineManager, Step};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct Feed {
    zip: Arc<Vec<u8>>,
    advertised_hash: String,
    base: Arc<Mutex<String>>,
    requests: Arc<Mutex<Vec<Option<String>>>>,
}

fn build_zip() -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("ollama.exe", opts).unwrap();
        // Pseudo-random bytes so the archive is big enough to interrupt.
        let mut x = 0x2545_F491_4F6C_DD1Du64;
        let data: Vec<u8> = (0..300_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect();
        zw.write_all(&data).unwrap();
        zw.start_file("lib/ollama/runner.dll", opts).unwrap();
        zw.write_all(b"dll").unwrap();
        zw.finish().unwrap();
    }
    buf.into_inner()
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

async fn release(State(f): State<Feed>) -> Json<serde_json::Value> {
    let base = f.base.lock().unwrap().clone();
    Json(json!({"tag_name":"v9.9.9","assets":[
        {"name":"ollama-windows-amd64.zip","browser_download_url":format!("{base}/zip"),"size":f.zip.len()},
        {"name":"sha256sum.txt","browser_download_url":format!("{base}/sums"),"size":10}]}))
}

async fn sums(State(f): State<Feed>) -> String {
    format!("{}  ollama-windows-amd64.zip\n", f.advertised_hash)
}

async fn zip_file(State(f): State<Feed>, headers: HeaderMap) -> impl IntoResponse {
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    f.requests.lock().unwrap().push(range.clone());
    match range.and_then(|r| {
        r.strip_prefix("bytes=")
            .and_then(|s| s.trim_end_matches('-').parse::<usize>().ok())
    }) {
        Some(start) if start < f.zip.len() => {
            (StatusCode::PARTIAL_CONTENT, f.zip[start..].to_vec()).into_response()
        }
        Some(_) => StatusCode::RANGE_NOT_SATISFIABLE.into_response(),
        None => (StatusCode::OK, f.zip.to_vec()).into_response(),
    }
}

async fn serve(zip: Vec<u8>, advertised_hash: String) -> (String, Feed) {
    let feed = Feed {
        zip: Arc::new(zip),
        advertised_hash,
        base: Arc::default(),
        requests: Arc::default(),
    };
    let app = Router::new()
        .route("/release", get(release))
        .route("/sums", get(sums))
        .route("/zip", get(zip_file))
        .with_state(feed.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    *feed.base.lock().unwrap() = base.clone();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (base, feed)
}

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("relay-engine-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn manager(root: &std::path::Path, base: &str) -> EngineManager {
    EngineManager::with_source(
        root.to_path_buf(),
        root.join("engine.log"),
        11999,
        format!("{base}/release"),
    )
}

#[tokio::test]
async fn installs_a_verified_engine_and_reports_progress() {
    let zip = build_zip();
    let (base, _feed) = serve(zip.clone(), hex(&zip)).await;
    let root = temp("ok");
    let m = manager(&root, &base);
    assert!(!m.is_installed());
    let mut steps: Vec<Step> = Vec::new();
    m.install(&mut |s| steps.push(s), &CancellationToken::new())
        .await
        .unwrap();
    assert!(m.is_installed());
    assert_eq!(
        std::fs::read(root.join("app/ollama.exe")).unwrap().len(),
        300_000
    );
    assert!(steps.iter().any(|s| s.label.starts_with("Downloading")));
    assert!(steps.iter().any(|s| s.label.starts_with("Verifying")));
    assert!(steps
        .iter()
        .any(|s| s.label.starts_with("Unpacking") && s.percent == Some(100)));
    assert!(
        !root.join("engine.zip.part").exists(),
        "temporary download is cleaned up"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("version.txt")).unwrap(),
        "v9.9.9"
    );
    // Installing again is a no-op.
    m.install(&mut |_| {}, &CancellationToken::new())
        .await
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn a_tampered_download_is_rejected_and_discarded() {
    let zip = build_zip();
    let (base, _feed) = serve(zip, "0".repeat(64)).await;
    let root = temp("bad");
    let m = manager(&root, &base);
    let err = m
        .install(&mut |_| {}, &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(matches!(err, EngineError::BadChecksum), "{err:?}");
    assert!(!m.is_installed());
    assert!(!root.join("engine.zip.part").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn an_interrupted_download_resumes_where_it_stopped() {
    let zip = build_zip();
    let (base, feed) = serve(zip.clone(), hex(&zip)).await;
    let root = temp("resume");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("engine.zip.part"), &zip[..1000]).unwrap();
    let m = manager(&root, &base);
    m.install(&mut |_| {}, &CancellationToken::new())
        .await
        .unwrap();
    assert!(m.is_installed());
    let reqs = feed.requests.lock().unwrap().clone();
    assert_eq!(reqs, vec![Some("bytes=1000-".to_string())]);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn cancelling_stops_the_download_and_keeps_the_partial_file() {
    let zip = build_zip();
    let (base, _feed) = serve(zip.clone(), hex(&zip)).await;
    let root = temp("cancel");
    let m = manager(&root, &base);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = m.install(&mut |_| {}, &cancel).await.unwrap_err();
    assert!(matches!(err, EngineError::Cancelled), "{err:?}");
    assert!(!m.is_installed());
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn network_failures_are_reported() {
    let root = temp("net");
    let m = manager(&root, "http://127.0.0.1:1");
    let err = m
        .install(&mut |_| {}, &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(matches!(err, EngineError::Network(_)), "{err:?}");
    assert!(!m.is_running().await);
    assert!(m.start().await.is_err());
    let _ = std::fs::remove_dir_all(&root);
}
