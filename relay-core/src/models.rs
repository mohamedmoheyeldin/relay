//! Installed local models: download, list, switch and delete (D-046).
//!
//! Layout: `<models dir>/<model id>/<files>` plus one manifest per installed quant,
//! `relay-<quant>.json`, written only after every file passed its SHA-256 check.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::catalog::{self, Fit, Model};
use crate::download::{self, Cancel};
use crate::gpu::Gpu;
use crate::settings::{ActiveModel, CustomModel, LocalAi};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub model_id: String,
    pub quant: String,
    pub model_file: String,
    pub mmproj_file: String,
    pub model_sha256: String,
    pub mmproj_sha256: String,
}

fn manifest_path(dir: &Path, quant: &str) -> PathBuf {
    dir.join(format!("relay-{quant}.json"))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Installed {
    pub model_id: String,
    pub quant: String,
    pub model_path: PathBuf,
    pub mmproj_path: PathBuf,
    pub bytes: u64,
}

impl Installed {
    pub fn active(&self) -> ActiveModel {
        ActiveModel {
            model_id: self.model_id.clone(),
            quant: self.quant.clone(),
            model_path: self.model_path.clone(),
            mmproj_path: self.mmproj_path.clone(),
        }
    }
}

fn file_len(path: &Path) -> u64 {
    path.metadata().map(|m| m.len()).unwrap_or(0)
}

/// Every installed model and quant in `models_dir`.
pub fn installed(models_dir: &Path) -> Vec<Installed> {
    let mut found = Vec::new();
    let Ok(dirs) = std::fs::read_dir(models_dir) else {
        return found;
    };
    for dir in dirs.flatten().filter(|d| d.path().is_dir()) {
        let Ok(files) = std::fs::read_dir(dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("relay-") && name.ends_with(".json")) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(file.path()) else {
                continue;
            };
            let Ok(m) = serde_json::from_str::<Manifest>(&text) else {
                continue;
            };
            let model_path = dir.path().join(&m.model_file);
            let mmproj_path = dir.path().join(&m.mmproj_file);
            if !model_path.is_file() || !mmproj_path.is_file() {
                continue;
            }
            found.push(Installed {
                bytes: file_len(&model_path) + file_len(&mmproj_path),
                model_id: m.model_id,
                quant: m.quant,
                model_path,
                mmproj_path,
            });
        }
    }
    found.sort_by(|a, b| (&a.model_id, &a.quant).cmp(&(&b.model_id, &b.quant)));
    found
}

/// Download a catalog model at `quant` (projector first, then weights).
pub async fn install(
    model: &Model,
    quant: &str,
    models_dir: &Path,
    cancel: &Cancel,
    on_progress: &(dyn Fn(String, u64, u64) + Send + Sync),
) -> Result<Installed> {
    let q = model
        .quant(quant)
        .with_context(|| format!("{} has no {quant} build", model.name))?
        .clone();
    let dir = models_dir.join(&model.id);
    let mmproj_dest = dir.join(&model.mmproj.file);
    let model_dest = dir.join(&q.file);
    let mmproj_size = model.mmproj.size;
    let total = q.size + mmproj_size;
    download::download(
        &model.url(&model.mmproj.file),
        &mmproj_dest,
        &model.mmproj.sha256,
        Some(mmproj_size),
        cancel,
        &|done, _| on_progress("Vision projector".into(), done, total),
    )
    .await?;
    download::download(
        &model.url(&q.file),
        &model_dest,
        &q.sha256,
        Some(q.size),
        cancel,
        &|done, _| on_progress("Model weights".into(), mmproj_size + done, total),
    )
    .await?;
    let manifest = Manifest {
        model_id: model.id.clone(),
        quant: quant.into(),
        model_file: q.file.clone(),
        mmproj_file: model.mmproj.file.clone(),
        model_sha256: q.sha256.clone(),
        mmproj_sha256: model.mmproj.sha256.clone(),
    };
    std::fs::write(
        manifest_path(&dir, quant),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(Installed {
        model_id: model.id.clone(),
        quant: quant.into(),
        bytes: file_len(&model_dest) + file_len(&mmproj_dest),
        model_path: model_dest,
        mmproj_path: mmproj_dest,
    })
}

/// Delete one installed quant. The projector goes too when no other quant uses it.
pub fn delete(models_dir: &Path, model_id: &str, quant: &str) -> Result<()> {
    let dir = models_dir.join(model_id);
    let manifest_file = manifest_path(&dir, quant);
    let text = std::fs::read_to_string(&manifest_file)
        .with_context(|| format!("{model_id} {quant} is not installed"))?;
    let m: Manifest = serde_json::from_str(&text)?;
    std::fs::remove_file(&manifest_file)?;
    let _ = std::fs::remove_file(dir.join(&m.model_file));
    let others = installed(models_dir)
        .into_iter()
        .any(|i| i.model_id == model_id && i.mmproj_path == dir.join(&m.mmproj_file));
    if !others {
        let _ = std::fs::remove_file(dir.join(&m.mmproj_file));
    }
    let _ = std::fs::remove_dir(&dir); // only succeeds once it is empty
    Ok(())
}

/// Turn a Hugging Face file link into (repo, file).
/// Accepts `.../<org>/<repo>/resolve/<rev>/<file>` and `.../blob/<rev>/<file>`.
pub fn parse_hf_url(url: &str) -> Result<(String, String, String)> {
    let rest = url
        .trim()
        .strip_prefix("https://huggingface.co/")
        .context("use a https://huggingface.co/... link")?;
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() < 5 || !(parts[2] == "resolve" || parts[2] == "blob") {
        bail!("use a link to a .gguf file (…/resolve/main/<file>.gguf)");
    }
    let file = parts[4..].join("/");
    let file = file.split('?').next().unwrap_or_default().to_string();
    if !file.ends_with(".gguf") {
        bail!("the link must point to a .gguf file");
    }
    Ok((
        format!("{}/{}", parts[0], parts[1]),
        parts[3].to_string(),
        file,
    ))
}

/// The SHA-256 Hugging Face reports for an LFS file (`x-linked-etag`), if any.
async fn hf_sha256(url: &str) -> Option<String> {
    let client = download::client().ok()?;
    let resp = client.head(url).send().await.ok()?;
    resp.headers()
        .get("x-linked-etag")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim_matches('"').to_string())
        .filter(|v| v.len() == 64)
}

/// Download a custom model and projector from Hugging Face links.
pub async fn install_custom_from_hf(
    name: &str,
    role: &str,
    model_url: &str,
    mmproj_url: &str,
    models_dir: &Path,
    cancel: &Cancel,
    on_progress: &(dyn Fn(String, u64, u64) + Send + Sync),
) -> Result<CustomModel> {
    let (repo, rev, model_file) = parse_hf_url(model_url)?;
    let (mrepo, mrev, mmproj_file) = parse_hf_url(mmproj_url)?;
    let id = format!("custom-{}", slug(name));
    let dir = models_dir.join(&id);
    let resolve = |r: &str, v: &str, f: &str| format!("https://huggingface.co/{r}/resolve/{v}/{f}");
    let model_dl = resolve(&repo, &rev, &model_file);
    let mmproj_dl = resolve(&mrepo, &mrev, &mmproj_file);
    let model_dest = dir.join(file_name(&model_file));
    let mmproj_dest = dir.join(file_name(&mmproj_file));
    let msha = hf_sha256(&mmproj_dl).await.unwrap_or_default();
    download::download(&mmproj_dl, &mmproj_dest, &msha, None, cancel, &|d, t| {
        on_progress("Vision projector".into(), d, t)
    })
    .await?;
    let sha = hf_sha256(&model_dl).await.unwrap_or_default();
    download::download(&model_dl, &model_dest, &sha, None, cancel, &|d, t| {
        on_progress("Model weights".into(), d, t)
    })
    .await?;
    Ok(CustomModel {
        id,
        name: name.into(),
        role: role.into(),
        model_path: model_dest,
        mmproj_path: mmproj_dest,
    })
}

/// Register a custom GGUF already on disk (it stays where it is).
pub fn custom_from_disk(
    name: &str,
    role: &str,
    model: &Path,
    mmproj: &Path,
) -> Result<CustomModel> {
    for (label, path) in [("model", model), ("projector", mmproj)] {
        if !path.is_file() {
            bail!("the {label} file {} does not exist", path.display());
        }
        if path.extension().and_then(|e| e.to_str()) != Some("gguf") {
            bail!("the {label} file must be a .gguf file");
        }
    }
    Ok(CustomModel {
        id: format!("custom-{}", slug(name)),
        name: name.into(),
        role: role.into(),
        model_path: model.to_path_buf(),
        mmproj_path: mmproj.to_path_buf(),
    })
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "model".into()
    } else {
        s
    }
}

/// One catalog entry as Settings > Local AI shows it.
#[derive(Debug, Clone, Serialize)]
pub struct QuantView {
    pub quant: String,
    pub download_bytes: u64,
    pub vram_bytes: u64,
    pub fit: Fit,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelView {
    pub id: String,
    pub name: String,
    pub role: String,
    pub summary: String,
    pub license: String,
    pub repo: String,
    pub default_quant: String,
    pub quants: Vec<QuantView>,
    pub recommended_quant: Option<String>,
    pub active_as: Vec<String>,
    pub benchmark: Option<catalog::BenchmarkResult>,
}

/// VRAM of whatever is active in the other role, for the "with the other model" fit.
fn other_need(local: &LocalAi, role: &str) -> u64 {
    let need = |a: &Option<ActiveModel>| {
        a.as_ref()
            .and_then(|a| catalog::find(&a.model_id).and_then(|m| m.vram_bytes(&a.quant)))
            .unwrap_or(0)
    };
    match role {
        "grounding" => need(&local.describe),
        "describe" => need(&local.grounding),
        _ => 0,
    }
}

pub fn views(local: &LocalAi, gpu: Option<&Gpu>) -> Vec<ModelView> {
    let installed = installed(&local.models_dir());
    let rec = catalog::recommend(gpu);
    let benches = catalog::benchmarks();
    catalog::models()
        .into_iter()
        .map(|m| {
            let other = other_need(local, &m.role);
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
                        installed: installed
                            .iter()
                            .any(|i| i.model_id == m.id && i.quant == q.quant),
                    }
                })
                .collect();
            let mut active_as = Vec::new();
            if local.grounding.as_ref().is_some_and(|a| a.model_id == m.id) {
                active_as.push("grounding".to_string());
            }
            if local.describe.as_ref().is_some_and(|a| a.model_id == m.id) {
                active_as.push("describe".to_string());
            }
            ModelView {
                recommended_quant: rec
                    .picks
                    .iter()
                    .find(|p| p.model_id == m.id)
                    .map(|p| p.quant.clone()),
                benchmark: benches.get(&m.id).cloned(),
                id: m.id,
                name: m.name,
                role: m.role,
                summary: m.summary,
                license: m.license,
                repo: m.repo,
                default_quant: m.default_quant,
                quants,
                active_as,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hf_links() {
        let (repo, rev, file) = parse_hf_url(
            "https://huggingface.co/Qwen/Qwen3-VL-8B-Instruct-GGUF/resolve/main/Qwen3VL-8B-Instruct-Q4_K_M.gguf?download=true",
        )
        .unwrap();
        assert_eq!(repo, "Qwen/Qwen3-VL-8B-Instruct-GGUF");
        assert_eq!(rev, "main");
        assert_eq!(file, "Qwen3VL-8B-Instruct-Q4_K_M.gguf");
        assert!(parse_hf_url("https://huggingface.co/Qwen/x/blob/main/a.gguf").is_ok());
        assert!(parse_hf_url("https://example.com/a.gguf").is_err());
        assert!(parse_hf_url("https://huggingface.co/Qwen/x/resolve/main/README.md").is_err());
    }

    #[test]
    fn installed_needs_a_manifest_and_files() {
        let dir = tempfile::tempdir().unwrap();
        let mdir = dir.path().join("holo2-4b");
        std::fs::create_dir_all(&mdir).unwrap();
        std::fs::write(mdir.join("w.gguf"), b"1234").unwrap();
        assert!(installed(dir.path()).is_empty());
        std::fs::write(mdir.join("p.gguf"), b"12").unwrap();
        let manifest = Manifest {
            model_id: "holo2-4b".into(),
            quant: "Q4_K_M".into(),
            model_file: "w.gguf".into(),
            mmproj_file: "p.gguf".into(),
            model_sha256: String::new(),
            mmproj_sha256: String::new(),
        };
        std::fs::write(
            manifest_path(&mdir, "Q4_K_M"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let found = installed(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bytes, 6);
        delete(dir.path(), "holo2-4b", "Q4_K_M").unwrap();
        assert!(!mdir.exists());
    }

    #[test]
    fn views_mark_recommendation() {
        let gpu = Gpu {
            name: "RTX".into(),
            vram_total_mb: 16_303,
            vram_free_mb: 14_000,
            driver: "1".into(),
        };
        let mut local = LocalAi::default();
        let tmp = tempfile::tempdir().unwrap();
        local.models_dir = Some(tmp.path().to_path_buf());
        let v = views(&local, Some(&gpu));
        let venus = v.iter().find(|m| m.id == "ui-venus-1.5-8b").unwrap();
        assert_eq!(venus.recommended_quant.as_deref(), Some("Q4_K_M"));
        assert!(v
            .iter()
            .find(|m| m.id == "holo2-4b")
            .unwrap()
            .recommended_quant
            .is_none());
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("My Model 2!"), "my-model-2");
        assert_eq!(slug("!!"), "model");
    }
}
