//! The pinned llama.cpp runtime (D-004): downloaded once into
//! `%LOCALAPPDATA%\Relay\runtime\llama-<build>`, checked by SHA-256, then unzipped.
//!
//! The CUDA 13 build needs an NVIDIA driver from the 580 series or newer (RTX 50 /
//! Blackwell). Older drivers and other GPUs get the Vulkan build.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::download::{self, Cancel};
use crate::gpu::Gpu;
use crate::paths;

pub const BUILD: &str = "b11624";

pub struct Asset {
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

const CUDA: &[Asset] = &[
    Asset {
        name: "llama-b11624-bin-win-cuda-13.4-x64.zip",
        size: 153_526_354,
        sha256: "c276a733f010a19a6448f16e31ab26d6776f3eda57d8b3f5db265405f6c98bf9",
    },
    Asset {
        name: "cudart-llama-bin-win-cuda-13.4-x64.zip",
        size: 423_535_356,
        sha256: "738f8c251ac22b70c3ae6f83a10cf222725df0395246a2cf58f32bdb85fbe668",
    },
];

const VULKAN: &[Asset] = &[Asset {
    name: "llama-b11624-bin-win-vulkan-x64.zip",
    size: 32_189_213,
    sha256: "0b4b6f70cbeee111b9ce06f77677222429e8b75e7262ee7e58ec75d15e4bc386",
}];

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Flavor {
    Cuda,
    Vulkan,
}

pub fn flavor_for(gpu: Option<&Gpu>) -> Flavor {
    let driver_major = gpu
        .and_then(|g| g.driver.split('.').next())
        .and_then(|m| m.parse::<u32>().ok())
        .unwrap_or(0);
    if driver_major >= 580 {
        Flavor::Cuda
    } else {
        Flavor::Vulkan
    }
}

pub fn assets(flavor: Flavor) -> &'static [Asset] {
    match flavor {
        Flavor::Cuda => CUDA,
        Flavor::Vulkan => VULKAN,
    }
}

pub fn download_bytes(flavor: Flavor) -> u64 {
    assets(flavor).iter().map(|a| a.size).sum()
}

pub fn install_dir() -> PathBuf {
    paths::runtime_dir().join(format!("llama-{BUILD}"))
}

fn find_server(dir: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    let direct = dir.join(exe);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .find_map(|e| find_server(&e.path()))
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeState {
    pub build: &'static str,
    pub installed: bool,
    pub llama_server: Option<PathBuf>,
    pub flavor: Option<Flavor>,
}

pub fn state() -> RuntimeState {
    let dir = install_dir();
    let flavor = std::fs::read_to_string(dir.join("relay-flavor.txt"))
        .ok()
        .and_then(|f| match f.trim() {
            "cuda" => Some(Flavor::Cuda),
            "vulkan" => Some(Flavor::Vulkan),
            _ => None,
        });
    let server = flavor.and(find_server(&dir));
    RuntimeState {
        build: BUILD,
        installed: server.is_some(),
        llama_server: server,
        flavor,
    }
}

fn unzip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut target = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut target)?;
    }
    Ok(())
}

/// Download and unpack the runtime. Zips are kept until the unpack succeeds.
pub async fn install(
    flavor: Flavor,
    cancel: &Cancel,
    on_progress: &(dyn Fn(String, u64, u64) + Send + Sync),
) -> Result<RuntimeState> {
    let dir = install_dir();
    let zips = paths::runtime_dir().join("downloads");
    let total = download_bytes(flavor);
    let mut before = 0;
    for asset in assets(flavor) {
        let url = format!(
            "https://github.com/ggml-org/llama.cpp/releases/download/{BUILD}/{}",
            asset.name
        );
        let dest = zips.join(asset.name);
        let offset = before;
        download::download(
            &url,
            &dest,
            asset.sha256,
            Some(asset.size),
            cancel,
            &|done, _| on_progress("Local AI runtime".into(), offset + done, total),
        )
        .await?;
        before += asset.size;
    }
    let dir_for_unzip = dir.clone();
    let zips_for_unzip = zips.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        std::fs::create_dir_all(&dir_for_unzip)?;
        for asset in assets(flavor) {
            unzip(&zips_for_unzip.join(asset.name), &dir_for_unzip)
                .with_context(|| format!("could not unpack {}", asset.name))?;
        }
        let tag = match flavor {
            Flavor::Cuda => "cuda",
            Flavor::Vulkan => "vulkan",
        };
        std::fs::write(dir_for_unzip.join("relay-flavor.txt"), tag)?;
        Ok(())
    })
    .await??;
    let _ = std::fs::remove_dir_all(&zips);
    Ok(state())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(driver: &str) -> Gpu {
        Gpu {
            name: "g".into(),
            vram_total_mb: 16_000,
            vram_free_mb: 16_000,
            driver: driver.into(),
        }
    }

    #[test]
    fn picks_flavor_by_driver() {
        assert_eq!(flavor_for(Some(&gpu("617.42"))), Flavor::Cuda);
        assert_eq!(flavor_for(Some(&gpu("560.10"))), Flavor::Vulkan);
        assert_eq!(flavor_for(None), Flavor::Vulkan);
    }

    #[test]
    fn unzips_into_place() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("a.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let opts: zip::write::SimpleFileOptions = Default::default();
            w.start_file("bin/llama-server", opts).unwrap();
            w.write_all(b"x").unwrap();
            w.finish().unwrap();
        }
        let out = dir.path().join("out");
        unzip(&zip_path, &out).unwrap();
        assert!(out.join("bin").join("llama-server").is_file());
        if !cfg!(windows) {
            assert_eq!(
                find_server(&out),
                Some(out.join("bin").join("llama-server"))
            );
        }
    }
}
