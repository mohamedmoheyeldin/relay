//! Relay's built-in local AI engine: downloads, verifies, unpacks and runs the model server so
//! the owner never installs anything by hand. Everything lives under Relay's data folder.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use relay_ai::process::kill_tree;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub const RELEASES_API: &str = "https://api.github.com/repos/ollama/ollama/releases/latest";
const ASSET_NAME: &str = "ollama-windows-amd64.zip";
const HASH_ASSET: &str = "sha256sum.txt";
pub const MANAGED_PORT: u16 = 11435;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("network problem: {0}")]
    Network(String),
    #[error("the download did not match its checksum; it was discarded")]
    BadChecksum,
    #[error("could not unpack the engine: {0}")]
    Unpack(String),
    #[error("the engine did not start: {0}")]
    Start(String),
    #[error("setup was cancelled")]
    Cancelled,
    #[error("no suitable download was found for this PC")]
    NoAsset,
    #[error("file error: {0}")]
    Io(String),
}

impl From<std::io::Error> for EngineError {
    fn from(e: std::io::Error) -> Self {
        EngineError::Io(e.to_string())
    }
}

/// A step the owner can see ("Downloading the engine", 42 %).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub label: String,
    pub percent: Option<u8>,
}

/// Where setup stands, shown on the Settings page.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SetupState {
    /// `idle`, `running`, `ready` or `failed`.
    pub state: String,
    pub stage: String,
    pub percent: Option<u8>,
    pub error: Option<String>,
}

impl SetupState {
    pub fn idle() -> Self {
        Self {
            state: "idle".into(),
            stage: String::new(),
            percent: None,
            error: None,
        }
    }
}

/// The graphics card, if it can be read.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GpuInfo {
    pub name: String,
    pub vram_mb: u64,
}

/// Parse `nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits` output.
pub fn parse_nvidia_smi(output: &str) -> Option<GpuInfo> {
    output
        .lines()
        .filter_map(|line| {
            let (name, mem) = line.rsplit_once(',')?;
            Some(GpuInfo {
                name: name.trim().to_string(),
                vram_mb: mem.trim().parse().ok()?,
            })
        })
        .max_by_key(|g| g.vram_mb)
}

pub async fn detect_gpu() -> Option<GpuInfo> {
    let mut cmd = Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=name,memory.total",
        "--format=csv,noheader,nounits",
    ])
    .stdin(Stdio::null())
    .stderr(Stdio::null())
    .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let out = tokio::time::timeout(Duration::from_secs(8), cmd.output())
        .await
        .ok()?
        .ok()?;
    parse_nvidia_smi(&String::from_utf8_lossy(&out.stdout))
}

/// The model to download for this hardware (strong tool use that fits in memory).
pub fn recommended_model(gpu: Option<&GpuInfo>) -> &'static str {
    match gpu.map(|g| g.vram_mb) {
        Some(mb) if mb >= 14_000 => "qwen3:14b",
        Some(mb) if mb >= 7_500 => "qwen3:8b",
        _ => "qwen3:4b",
    }
}

/// Pick the Windows engine zip and its checksum from a GitHub release document.
pub fn pick_assets(release: &Value) -> Option<(String, u64, String, Option<String>)> {
    let assets = release.get("assets")?.as_array()?;
    let find = |name: &str| {
        assets
            .iter()
            .find(|a| a.get("name").and_then(Value::as_str) == Some(name))
    };
    let zip = find(ASSET_NAME)?;
    let url = zip.get("browser_download_url")?.as_str()?.to_string();
    let size = zip.get("size").and_then(Value::as_u64).unwrap_or(0);
    let tag = release
        .get("tag_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let hash_url = find(HASH_ASSET)
        .and_then(|a| a.get("browser_download_url"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Some((url, size, tag, hash_url))
}

/// Find the hash for `file` in a `sha256sum.txt` body (`<hash>  <name>` lines).
pub fn find_checksum(body: &str, file: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == file && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Unpack a zip into `dest`, refusing paths that escape it.
pub fn extract_zip(
    zip_path: &Path,
    dest: &Path,
    mut progress: impl FnMut(u8),
) -> Result<(), EngineError> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| EngineError::Unpack(e.to_string()))?;
    std::fs::create_dir_all(dest)?;
    let total = archive.len().max(1);
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| EngineError::Unpack(e.to_string()))?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(EngineError::Unpack(format!(
                "unsafe path in archive: {}",
                entry.name().map(|n| n.into_owned()).unwrap_or_default()
            )));
        };
        let out = dest.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut target = std::fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut target)?;
        }
        progress(((i + 1) * 100 / total) as u8);
    }
    Ok(())
}

/// Owns the engine's files and process.
pub struct EngineManager {
    root: PathBuf,
    models_dir: PathBuf,
    log_path: PathBuf,
    port: u16,
    releases_api: String,
    http: reqwest::Client,
    child: Mutex<Option<Child>>,
    state: Mutex<SetupState>,
}

impl EngineManager {
    pub fn new(root: PathBuf, log_path: PathBuf, port: u16) -> Self {
        Self::with_source(root, log_path, port, RELEASES_API.to_string())
    }

    /// Like [`EngineManager::new`] but with a custom release feed (tests).
    pub fn with_source(root: PathBuf, log_path: PathBuf, port: u16, releases_api: String) -> Self {
        Self {
            models_dir: root.join("models"),
            root,
            log_path,
            port,
            releases_api,
            http: reqwest::Client::builder()
                .user_agent("Relay-Operator")
                .connect_timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            child: Mutex::new(None),
            state: Mutex::new(SetupState::idle()),
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn app_dir(&self) -> PathBuf {
        self.root.join("app")
    }

    fn exe(&self) -> PathBuf {
        self.app_dir().join("ollama.exe")
    }

    pub fn is_installed(&self) -> bool {
        self.exe().is_file()
    }

    pub fn state(&self) -> SetupState {
        self.state
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|p| p.into_inner().clone())
    }

    pub fn set_state(&self, state: SetupState) {
        if let Ok(mut s) = self.state.lock() {
            *s = state;
        }
    }

    async fn get_json(&self, url: &str) -> Result<Value, EngineError> {
        self.http
            .get(url)
            .send()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| EngineError::Network(e.to_string()))?
            .json()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))
    }

    /// Download (resuming if interrupted), verify and unpack the engine.
    pub async fn install(
        &self,
        on_step: &mut (dyn FnMut(Step) + Send),
        cancel: &CancellationToken,
    ) -> Result<(), EngineError> {
        if self.is_installed() {
            return Ok(());
        }
        on_step(Step {
            label: "Finding the latest engine".into(),
            percent: None,
        });
        let release = self.get_json(&self.releases_api).await?;
        let (url, size, tag, hash_url) = pick_assets(&release).ok_or(EngineError::NoAsset)?;
        let expected = match hash_url {
            Some(u) => {
                let body = self
                    .http
                    .get(&u)
                    .send()
                    .await
                    .map_err(|e| EngineError::Network(e.to_string()))?
                    .text()
                    .await
                    .map_err(|e| EngineError::Network(e.to_string()))?;
                find_checksum(&body, ASSET_NAME)
            }
            None => None,
        };
        tokio::fs::create_dir_all(&self.root).await?;
        let part = self.root.join("engine.zip.part");
        self.download(&url, size, &part, on_step, cancel).await?;

        on_step(Step {
            label: "Verifying the download".into(),
            percent: None,
        });
        let part_for_hash = part.clone();
        let actual = tokio::task::spawn_blocking(move || sha256_file(&part_for_hash))
            .await
            .map_err(|e| EngineError::Io(e.to_string()))??;
        match expected {
            Some(want) if want != actual => {
                let _ = tokio::fs::remove_file(&part).await;
                return Err(EngineError::BadChecksum);
            }
            Some(_) => {}
            None => warn!("no checksum published for {tag}; continuing without one"),
        }

        let staging = self.root.join("app.staging");
        let _ = tokio::fs::remove_dir_all(&staging).await;
        let (zip_path, dest) = (part.clone(), staging.clone());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<u8>();
        let unpack = tokio::task::spawn_blocking(move || {
            extract_zip(&zip_path, &dest, |p| {
                let _ = tx.send(p);
            })
        });
        while let Some(p) = rx.recv().await {
            on_step(Step {
                label: "Unpacking the engine".into(),
                percent: Some(p),
            });
        }
        unpack
            .await
            .map_err(|e| EngineError::Unpack(e.to_string()))??;
        let app = self.app_dir();
        let _ = tokio::fs::remove_dir_all(&app).await;
        tokio::fs::rename(&staging, &app).await?;
        let _ = tokio::fs::remove_file(&part).await;
        tokio::fs::write(self.root.join("version.txt"), &tag).await?;
        info!("local AI engine {tag} installed");
        Ok(())
    }

    async fn download(
        &self,
        url: &str,
        expected_size: u64,
        part: &Path,
        on_step: &mut (dyn FnMut(Step) + Send),
        cancel: &CancellationToken,
    ) -> Result<(), EngineError> {
        let have = tokio::fs::metadata(part)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let mut request = self.http.get(url);
        if have > 0 {
            request = request.header("Range", format!("bytes={have}-"));
        }
        let mut response = request
            .send()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))?;
        let status = response.status();
        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            return Ok(()); // the partial file is already complete
        }
        if !status.is_success() {
            return Err(EngineError::Network(format!("server answered {status}")));
        }
        let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut file = if resumed {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(part)
                .await?
        } else {
            tokio::fs::File::create(part).await?
        };
        let mut done = if resumed { have } else { 0 };
        let total = if expected_size > 0 {
            expected_size
        } else {
            done + response.content_length().unwrap_or(0)
        };
        let mut last_percent = 255u8;
        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = file.flush().await;
                    return Err(EngineError::Cancelled);
                }
                c = response.chunk() => c.map_err(|e| EngineError::Network(e.to_string()))?,
            };
            let Some(chunk) = chunk else { break };
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            let percent = (total > 0).then(|| ((done * 100) / total).min(100) as u8);
            if percent.unwrap_or(0) != last_percent {
                last_percent = percent.unwrap_or(0);
                on_step(Step {
                    label: format!(
                        "Downloading the engine ({} of {} MB)",
                        done / 1_000_000,
                        total / 1_000_000
                    ),
                    percent,
                });
            }
        }
        file.flush().await?;
        Ok(())
    }

    /// Whether the engine's server answers.
    pub async fn is_running(&self) -> bool {
        self.http
            .get(format!("{}/api/tags", self.base_url()))
            .timeout(Duration::from_millis(800))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Start the server (hidden, with models kept in Relay's folder) and wait until it answers.
    pub async fn start(&self) -> Result<(), EngineError> {
        if self.is_running().await {
            return Ok(());
        }
        if !self.is_installed() {
            return Err(EngineError::Start("the engine is not installed".into()));
        }
        tokio::fs::create_dir_all(&self.models_dir).await?;
        if let Some(parent) = self.log_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;
        let log_err = log.try_clone()?;
        let mut cmd = Command::new(self.exe());
        cmd.arg("serve")
            .env("OLLAMA_HOST", format!("127.0.0.1:{}", self.port))
            .env("OLLAMA_MODELS", &self.models_dir)
            .env("OLLAMA_KEEP_ALIVE", "30m")
            .env("OLLAMA_NOHISTORY", "1")
            .current_dir(self.app_dir())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err))
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        let child = cmd.spawn().map_err(|e| EngineError::Start(e.to_string()))?;
        if let Ok(mut slot) = self.child.lock() {
            *slot = Some(child);
        }
        for _ in 0..120 {
            if self.is_running().await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Err(EngineError::Start(
            "the server did not answer within 30 seconds".into(),
        ))
    }

    /// Stop the server and everything it started.
    pub async fn stop(&self) {
        let child = self.child.lock().ok().and_then(|mut c| c.take());
        if let Some(mut child) = child {
            kill_tree(&mut child).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nvidia_output_is_parsed_and_the_biggest_card_wins() {
        let out = "NVIDIA GeForce RTX 5080, 16303\nNVIDIA T400, 4096\n";
        assert_eq!(
            parse_nvidia_smi(out),
            Some(GpuInfo {
                name: "NVIDIA GeForce RTX 5080".into(),
                vram_mb: 16303
            })
        );
        assert_eq!(parse_nvidia_smi(""), None);
        assert_eq!(parse_nvidia_smi("garbage"), None);
    }

    #[test]
    fn model_recommendation_follows_memory() {
        let gpu = |mb| GpuInfo {
            name: "x".into(),
            vram_mb: mb,
        };
        assert_eq!(recommended_model(Some(&gpu(16303))), "qwen3:14b");
        assert_eq!(recommended_model(Some(&gpu(8192))), "qwen3:8b");
        assert_eq!(recommended_model(Some(&gpu(4096))), "qwen3:4b");
        assert_eq!(recommended_model(None), "qwen3:4b");
    }

    #[test]
    fn release_assets_and_checksums_are_found() {
        let release = json!({"tag_name":"v1.2.3","assets":[
            {"name":"ollama-windows-arm64.zip","browser_download_url":"https://x/arm","size":1},
            {"name":"ollama-windows-amd64.zip","browser_download_url":"https://x/amd","size":99},
            {"name":"sha256sum.txt","browser_download_url":"https://x/sums","size":5}]});
        let (url, size, tag, hash) = pick_assets(&release).unwrap();
        assert_eq!(
            (url.as_str(), size, tag.as_str(), hash.as_deref()),
            ("https://x/amd", 99, "v1.2.3", Some("https://x/sums"))
        );
        assert!(pick_assets(&json!({"assets":[]})).is_none());
        let sums = format!(
            "{}  ollama-linux.tgz\n{}  ollama-windows-amd64.zip\n",
            "a".repeat(64),
            "B".repeat(64)
        );
        assert_eq!(
            find_checksum(&sums, "ollama-windows-amd64.zip"),
            Some("b".repeat(64))
        );
        assert_eq!(find_checksum(&sums, "nope.zip"), None);
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("relay-engine-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    pub(crate) fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        use std::io::Write;
        let file = std::fs::File::create(path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
    }

    #[test]
    fn zips_extract_and_hash() {
        let dir = temp("zip");
        let zip_path = dir.join("a.zip");
        make_zip(
            &zip_path,
            &[("ollama.exe", b"binary"), ("lib/cuda/x.dll", b"dll")],
        );
        let mut last = 0;
        extract_zip(&zip_path, &dir.join("out"), |p| last = p).unwrap();
        assert_eq!(last, 100);
        assert_eq!(
            std::fs::read(dir.join("out/ollama.exe")).unwrap(),
            b"binary"
        );
        assert_eq!(
            std::fs::read(dir.join("out/lib/cuda/x.dll")).unwrap(),
            b"dll"
        );
        let hash = sha256_file(&dir.join("out/ollama.exe")).unwrap();
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, sha256_file(&dir.join("out/ollama.exe")).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn archives_cannot_escape_the_target_folder() {
        let dir = temp("evil");
        let zip_path = dir.join("evil.zip");
        make_zip(&zip_path, &[("../escape.txt", b"x")]);
        assert!(matches!(
            extract_zip(&zip_path, &dir.join("out"), |_| {}),
            Err(EngineError::Unpack(_))
        ));
        assert!(!dir.join("escape.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
