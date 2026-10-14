//! GPU and VRAM detection through `nvidia-smi` (installed with the NVIDIA driver).

use serde::Serialize;

use crate::proc;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Gpu {
    pub name: String,
    pub vram_total_mb: u64,
    pub vram_free_mb: u64,
    pub driver: String,
}

/// Parse `nvidia-smi --query-gpu=name,memory.total,memory.free,driver_version
/// --format=csv,noheader,nounits`: one line per GPU.
pub fn parse_nvidia_smi(text: &str) -> Vec<Gpu> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split(',').map(str::trim).collect();
            if parts.len() < 4 {
                return None;
            }
            Some(Gpu {
                name: parts[0].to_string(),
                vram_total_mb: parts[1].parse().ok()?,
                vram_free_mb: parts[2].parse().ok()?,
                driver: parts[3].to_string(),
            })
        })
        .collect()
}

fn nvidia_smi() -> std::path::PathBuf {
    if let Some(found) = proc::which("nvidia-smi") {
        return found;
    }
    let system = std::path::PathBuf::from(r"C:\Windows\System32\nvidia-smi.exe");
    if system.is_file() {
        system
    } else {
        "nvidia-smi".into()
    }
}

/// The NVIDIA GPU with the most VRAM, or None (no NVIDIA GPU or driver).
pub async fn detect() -> Option<Gpu> {
    let out = proc::run(
        nvidia_smi(),
        &[
            "--query-gpu=name,memory.total,memory.free,driver_version",
            "--format=csv,noheader,nounits",
        ],
    )
    .await
    .ok()?;
    if !out.ok {
        return None;
    }
    parse_nvidia_smi(&out.stdout)
        .into_iter()
        .max_by_key(|g| g.vram_total_mb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_csv() {
        let gpus = parse_nvidia_smi("NVIDIA GeForce RTX 5080, 16303, 13512, 581.57\n");
        assert_eq!(
            gpus,
            vec![Gpu {
                name: "NVIDIA GeForce RTX 5080".into(),
                vram_total_mb: 16303,
                vram_free_mb: 13512,
                driver: "581.57".into(),
            }]
        );
        assert!(parse_nvidia_smi("garbage").is_empty());
    }
}
