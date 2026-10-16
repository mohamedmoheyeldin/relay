//! Settings > Local AI on top of relay-screen's model runtime (M1-09, D-052).
//!
//! relay-screen owns the llama.cpp runtime and the model files: it resolves each
//! download from the publisher, verifies the SHA-256 and records it in
//! `installed.json`, which its supervisor loads from. The app runs the bundled
//! relay-screen's command line for that (`models list --json`, `models download`,
//! `runtime install`), so there is one store and one set of rules. The app adds the
//! fit check, the recommendation and which model plays which role.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

use crate::catalog::{self, Fit};
use crate::download::Cancel;
use crate::gpu::Gpu;
use crate::proc;
use crate::settings::{ActiveModel, LocalAi};

/// A catalog entry as relay-screen knows it; `repo` is None until a trusted source
/// is picked (the GUI models get theirs with the M1-11 benchmark).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub repo: Option<String>,
    pub quant: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstalledModel {
    pub id: String,
    pub repo: String,
    pub revision: String,
    pub quant: String,
    pub size: u64,
    /// Every file is on disk.
    pub ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeInfo {
    pub tag: String,
    pub cuda: String,
    pub server: PathBuf,
}

/// `relay-screen models list --json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Listing {
    pub home: PathBuf,
    pub models_folder: PathBuf,
    pub runtime: Option<RuntimeInfo>,
    pub catalog: Vec<Source>,
    pub installed: Vec<InstalledModel>,
}

impl Listing {
    pub fn installed(&self, id: &str) -> Option<&InstalledModel> {
        self.installed.iter().find(|m| m.id == id && m.ready)
    }
}

/// The console Python next to the bundled `pythonw.exe`, so the CLI's output can be read.
pub fn cli_python(python: &Path) -> PathBuf {
    let console = python.with_file_name(if cfg!(windows) {
        "python.exe"
    } else {
        "python3"
    });
    if console.is_file() {
        console
    } else {
        python.to_path_buf()
    }
}

fn cli(python: &Path, args: &[&str]) -> tokio::process::Command {
    let mut cmd = proc::command(cli_python(python));
    cmd.args(["-m", "relay_screen.server"])
        .args(args)
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8");
    cmd
}

/// The last line relay-screen wrote to stderr, without its "Error: " prefix.
fn error_text(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.strip_prefix("Error: ").unwrap_or(l).to_string())
        .unwrap_or_else(|| "relay-screen stopped without saying why".into())
}

pub async fn list(python: &Path) -> Result<Listing> {
    let out = cli(python, &["models", "list", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("could not start relay-screen")?;
    if !out.status.success() {
        bail!(error_text(&String::from_utf8_lossy(&out.stderr)));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.trim()).context("relay-screen gave an unexpected model list")
}

#[derive(Deserialize)]
struct ProgressLine {
    done: u64,
    total: Option<u64>,
    file: usize,
}

/// Run a relay-screen job with `--json` progress. Cancelling stops it; a later run
/// resumes from the partial file.
async fn run_job(
    python: &Path,
    args: &[&str],
    stages: &[&str],
    cancel: &Cancel,
    on_progress: &(dyn Fn(String, u64, u64) + Send + Sync),
) -> Result<()> {
    let mut child = cli(python, args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("could not start relay-screen")?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let err_task = tokio::spawn(async move {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text).await;
        text
    });
    let mut lines = BufReader::new(stdout).lines();
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    loop {
        tokio::select! {
            line = lines.next_line() => match line? {
                Some(line) => {
                    if let Ok(p) = serde_json::from_str::<ProgressLine>(&line) {
                        let stage = stages.get(p.file).copied().unwrap_or("Downloading");
                        on_progress(stage.into(), p.done, p.total.unwrap_or(0));
                    }
                }
                None => break,
            },
            _ = tick.tick() => {
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.kill().await;
                    bail!("Cancelled. The download picks up where it stopped next time.");
                }
            }
        }
    }
    let status = child.wait().await?;
    let stderr = err_task.await.unwrap_or_default();
    if !status.success() {
        bail!(error_text(&stderr));
    }
    Ok(())
}

/// Download and verify llama.cpp (the newest CUDA build the driver supports).
pub async fn install_runtime(
    python: &Path,
    cancel: &Cancel,
    on_progress: &(dyn Fn(String, u64, u64) + Send + Sync),
) -> Result<()> {
    run_job(
        python,
        &["runtime", "install", "--json"],
        &["llama.cpp", "CUDA runtime"],
        cancel,
        on_progress,
    )
    .await
}

/// Download a catalog model (by id) or a Hugging Face GGUF repo (`org/name`).
pub async fn download(
    python: &Path,
    model: &str,
    quant: &str,
    cancel: &Cancel,
    on_progress: &(dyn Fn(String, u64, u64) + Send + Sync),
) -> Result<()> {
    run_job(
        python,
        &["models", "download", model, "--quant", quant, "--json"],
        &["Model weights", "Vision projector"],
        cancel,
        on_progress,
    )
    .await
}

pub async fn remove(python: &Path, model_id: &str) -> Result<()> {
    let out = cli(python, &["models", "remove", model_id])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("could not start relay-screen")?;
    if !out.status.success() {
        bail!(error_text(&String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

/// The id relay-screen gives a model added by Hugging Face repo.
pub fn custom_id(repo: &str) -> String {
    let name = repo.rsplit('/').next().unwrap_or(repo).to_lowercase();
    name.strip_suffix("-gguf").unwrap_or(&name).to_string()
}

/// Check a Hugging Face repo name (`org/name`) before downloading from it.
pub fn check_repo(repo: &str) -> Result<String> {
    let repo = repo
        .trim()
        .trim_start_matches("https://huggingface.co/")
        .trim_end_matches('/');
    let parts: Vec<&str> = repo.split('/').collect();
    let ok = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    if parts.len() != 2 || !parts.iter().all(|p| ok(p)) {
        bail!("use a Hugging Face repo name like Qwen/Qwen3-VL-8B-Instruct-GGUF");
    }
    Ok(repo.to_string())
}

// ------------------------------------------------------------------ page views

#[derive(Debug, Clone, Serialize)]
pub struct QuantView {
    pub quant: String,
    pub download_bytes: u64,
    pub vram_bytes: u64,
    pub fit: Fit,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelView {
    pub id: String,
    pub name: String,
    pub role: String,
    pub summary: String,
    pub license: String,
    /// The Hugging Face repo relay-screen downloads from; None until one is picked.
    pub source: Option<String>,
    pub default_quant: String,
    pub quants: Vec<QuantView>,
    pub installed: Option<InstalledModel>,
    pub recommended_quant: Option<String>,
    pub active_as: Vec<String>,
    pub benchmark: Option<catalog::BenchmarkResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CustomView {
    pub model: InstalledModel,
    pub fit: Fit,
    pub active_as: Vec<String>,
}

fn active_as(local: &LocalAi, id: &str) -> Vec<String> {
    let mut roles = Vec::new();
    if local.grounding.as_ref().is_some_and(|a| a.model_id == id) {
        roles.push("grounding".to_string());
    }
    if local.describe.as_ref().is_some_and(|a| a.model_id == id) {
        roles.push("describe".to_string());
    }
    roles
}

/// Estimated VRAM of a model by id: the catalog estimate, else its size on disk.
fn need(active: &ActiveModel, listing: Option<&Listing>) -> u64 {
    if let Some(n) = catalog::find(&active.model_id).and_then(|m| m.vram_bytes(&active.quant)) {
        return n;
    }
    listing
        .and_then(|l| l.installed(&active.model_id))
        .map(|m| catalog::vram_for_size(m.size))
        .unwrap_or(0)
}

/// VRAM of whatever is active in the other role, for the "next to the other model" fit.
fn other_need(local: &LocalAi, role: &str, listing: Option<&Listing>) -> u64 {
    let slot = match role {
        "grounding" => &local.describe,
        "describe" => &local.grounding,
        _ => return 0,
    };
    slot.as_ref().map(|a| need(a, listing)).unwrap_or(0)
}

pub fn views(
    local: &LocalAi,
    gpu: Option<&Gpu>,
    listing: Option<&Listing>,
) -> (Vec<ModelView>, Vec<CustomView>) {
    let rec = catalog::recommend(gpu);
    let benches = catalog::benchmarks();
    let models: Vec<ModelView> = catalog::models()
        .into_iter()
        .map(|m| {
            let other = other_need(local, &m.role, listing);
            let quants = m
                .quants
                .iter()
                .map(|q| {
                    let vram = m.vram_bytes(&q.quant).unwrap_or(0);
                    QuantView {
                        quant: q.quant.clone(),
                        download_bytes: m.download_bytes(&q.quant).unwrap_or(0),
                        vram_bytes: vram,
                        fit: catalog::fit(gpu, vram, other),
                    }
                })
                .collect();
            ModelView {
                source: listing.and_then(|l| {
                    l.catalog
                        .iter()
                        .find(|s| s.id == m.id)
                        .and_then(|s| s.repo.clone())
                }),
                installed: listing.and_then(|l| l.installed.iter().find(|i| i.id == m.id).cloned()),
                recommended_quant: rec
                    .picks
                    .iter()
                    .find(|p| p.model_id == m.id)
                    .map(|p| p.quant.clone()),
                benchmark: benches.get(&m.id).cloned(),
                active_as: active_as(local, &m.id),
                id: m.id,
                name: m.name,
                role: m.role,
                summary: m.summary,
                license: m.license,
                default_quant: m.default_quant,
                quants,
            }
        })
        .collect();
    let custom = listing
        .map(|l| {
            l.installed
                .iter()
                .filter(|i| catalog::find(&i.id).is_none())
                .map(|i| CustomView {
                    fit: catalog::fit(gpu, catalog::vram_for_size(i.size), 0),
                    active_as: active_as(local, &i.id),
                    model: i.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    (models, custom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing() -> Listing {
        serde_json::from_str(
            r#"{
              "home": "C:\\Relay", "models_folder": "C:\\Relay\\models",
              "runtime": {"tag": "b1", "cuda": "13.1", "server": "C:\\Relay\\runtime\\b1\\llama-server.exe"},
              "catalog": [
                {"id": "ui-venus-1.5-8b", "name": "UI-Venus-1.5-8B", "role": "locate", "repo": null,
                 "quant": "Q4_K_M", "context": 8192, "license": "Apache-2.0"},
                {"id": "qwen3-vl-8b-instruct", "name": "Qwen3-VL-8B-Instruct", "role": "check",
                 "repo": "Qwen/Qwen3-VL-8B-Instruct-GGUF", "quant": "Q4_K_M", "context": 8192,
                 "license": "Apache-2.0"}
              ],
              "defaults": {"locate": "ui-venus-1.5-8b", "check": "qwen3-vl-8b-instruct"},
              "installed": [
                {"id": "qwen3-vl-8b-instruct", "repo": "Qwen/Qwen3-VL-8B-Instruct-GGUF",
                 "revision": "abc", "quant": "Q4_K_M", "size": 6200000000, "ready": true},
                {"id": "my-model", "repo": "Me/My-Model-GGUF", "revision": "def",
                 "quant": "Q5_K_M", "size": 3000000000, "ready": true}
              ]
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn views_join_catalog_and_installed() {
        let gpu = Gpu {
            name: "RTX".into(),
            vram_total_mb: 16_303,
            vram_free_mb: 14_000,
            driver: "1".into(),
        };
        let local = LocalAi {
            describe: Some(ActiveModel {
                model_id: "qwen3-vl-8b-instruct".into(),
                quant: "Q4_K_M".into(),
            }),
            ..LocalAi::default()
        };
        let l = listing();
        let (models, custom) = views(&local, Some(&gpu), Some(&l));
        let venus = models.iter().find(|m| m.id == "ui-venus-1.5-8b").unwrap();
        assert_eq!(venus.source, None);
        assert_eq!(venus.recommended_quant, None);
        let holo = models.iter().find(|m| m.id == "holo-3.1-9b").unwrap();
        assert_eq!(holo.recommended_quant.as_deref(), Some("Q5_K_M"));
        assert_eq!(venus.quants[0].fit, Fit::Fits);
        let qwen = models
            .iter()
            .find(|m| m.id == "qwen3-vl-8b-instruct")
            .unwrap();
        assert_eq!(
            qwen.source.as_deref(),
            Some("Qwen/Qwen3-VL-8B-Instruct-GGUF")
        );
        assert_eq!(qwen.installed.as_ref().unwrap().quant, "Q4_K_M");
        assert_eq!(qwen.active_as, ["describe"]);
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].model.id, "my-model");
        assert!(views(&local, None, None).1.is_empty());
    }

    #[test]
    fn custom_ids_match_relay_screen() {
        assert_eq!(custom_id("Me/My-Model-GGUF"), "my-model");
        assert_eq!(custom_id("Org/thing"), "thing");
    }

    #[test]
    fn repo_names() {
        assert_eq!(
            check_repo("https://huggingface.co/Qwen/Qwen3-VL-8B-Instruct-GGUF/").unwrap(),
            "Qwen/Qwen3-VL-8B-Instruct-GGUF"
        );
        assert!(check_repo("Qwen").is_err());
        assert!(check_repo("a/b/c").is_err());
        assert!(check_repo("a/b c").is_err());
    }

    #[test]
    fn errors_come_from_the_last_stderr_line() {
        assert_eq!(
            error_text("Traceback\nError: Not enough disk space\n\n"),
            "Not enough disk space"
        );
        assert_eq!(error_text(""), "relay-screen stopped without saying why");
    }
}
