//! The local model catalog, fit check and recommendation (D-046, D-036).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::gpu::Gpu;

const CATALOG_JSON: &str = include_str!("../catalog/models.json");

const GB: f64 = 1024.0 * 1024.0 * 1024.0;
/// KV cache and compute buffers on top of the weights, at Relay's context size.
const RUNTIME_OVERHEAD_BYTES: u64 = 1_200 * 1024 * 1024;
/// VRAM left for Windows, the browser and other apps.
const DESKTOP_RESERVE_MB: u64 = 1_536;

/// One quantization with its approximate weights size, for the fit check.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Quant {
    pub quant: String,
    pub size: u64,
}

/// A catalog model as the app describes it. Where it downloads from, and the
/// checksums, come from relay-screen's runtime at download time (D-052).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Model {
    pub id: String,
    pub name: String,
    /// "grounding", "describe" or "both".
    pub role: String,
    pub summary: String,
    pub license: String,
    pub default_quant: String,
    pub quants: Vec<Quant>,
    /// Approximate size of the full-precision vision projector.
    pub mmproj_size: u64,
}

#[derive(Deserialize)]
struct CatalogFile {
    models: Vec<Model>,
}

pub fn models() -> Vec<Model> {
    serde_json::from_str::<CatalogFile>(CATALOG_JSON)
        .expect("catalog/models.json is valid")
        .models
}

pub fn find(id: &str) -> Option<Model> {
    models().into_iter().find(|m| m.id == id)
}

impl Model {
    pub fn quant(&self, quant: &str) -> Option<&Quant> {
        self.quants.iter().find(|q| q.quant == quant)
    }

    /// Approximate download size of the weights plus the vision projector.
    pub fn download_bytes(&self, quant: &str) -> Option<u64> {
        self.quant(quant).map(|q| q.size + self.mmproj_size)
    }

    /// Estimated VRAM to run this model at `quant`.
    pub fn vram_bytes(&self, quant: &str) -> Option<u64> {
        self.download_bytes(quant)
            .map(|b| b + RUNTIME_OVERHEAD_BYTES)
    }
}

/// Estimated VRAM for a model of `size` bytes on disk (custom models).
pub fn vram_for_size(size: u64) -> u64 {
    size + RUNTIME_OVERHEAD_BYTES
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// Fits next to the other resident model.
    Fits,
    /// Fits on its own, not with the other model loaded.
    Tight,
    WontFit,
    /// No NVIDIA GPU was found.
    NoGpu,
}

pub fn budget_bytes(gpu: &Gpu) -> u64 {
    gpu.vram_total_mb.saturating_sub(DESKTOP_RESERVE_MB) * 1024 * 1024
}

/// Fit of a model that needs `need` bytes next to another resident one needing `other`.
pub fn fit(gpu: Option<&Gpu>, need: u64, other: u64) -> Fit {
    let Some(gpu) = gpu else { return Fit::NoGpu };
    let budget = budget_bytes(gpu);
    if need + other <= budget {
        Fit::Fits
    } else if need <= budget {
        Fit::Tight
    } else {
        Fit::WontFit
    }
}

/// One model and quant Relay suggests for a role.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Pick {
    pub model_id: String,
    pub quant: String,
    /// "grounding", "describe" or "both".
    pub role: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Recommendation {
    pub picks: Vec<Pick>,
    pub reason: String,
}

fn pick(id: &str, quant: &str, role: &str) -> Pick {
    Pick {
        model_id: id.into(),
        quant: quant.into(),
        role: role.into(),
    }
}

/// D-036's resident pair when it fits, else one model for both roles, else nothing.
/// Benchmark scores (M1-11) will refine this once they exist.
pub fn recommend(gpu: Option<&Gpu>) -> Recommendation {
    let Some(gpu) = gpu else {
        return Recommendation {
            picks: vec![],
            reason:
                "No NVIDIA GPU found. Relay still works: Cloud AI looks at the screenshots itself."
                    .into(),
        };
    };
    let need = |id: &str, q: &str| {
        find(id)
            .and_then(|m| m.vram_bytes(q))
            .unwrap_or(u64::MAX / 4)
    };
    let budget = budget_bytes(gpu);
    let gb = |b: u64| b as f64 / GB;
    let main = need("holo-3.1-9b", "Q5_K_M");
    if main <= budget {
        return Recommendation {
            picks: vec![pick("holo-3.1-9b", "Q5_K_M", "both")],
            reason: format!(
                "Holo3.1-9B finds things on screen and plans the steps. It stays loaded in about {:.1} GB of your {}'s {:.1} GB, so tasks never wait for a model to load.",
                gb(main),
                gpu.name,
                gpu.vram_total_mb as f64 / 1024.0
            ),
        };
    }
    for (id, label) in [("holo2-8b", "Holo2-8B"), ("holo2-4b", "Holo2-4B")] {
        let n = need(id, "Q4_K_M");
        if n <= budget {
            return Recommendation {
                picks: vec![pick(id, "Q4_K_M", "both")],
                reason: format!(
                    "Holo3.1-9B needs about {:.1} GB, more than fits here, so {label} does the work in about {:.1} GB.",
                    gb(main),
                    gb(n)
                ),
            };
        }
    }
    Recommendation {
        picks: vec![],
        reason: format!(
            "{} has too little VRAM for a local model. Relay still works: Cloud AI looks at the screenshots itself.",
            gpu.name
        ),
    }
}

/// Benchmark results per model id, written by Relay's benchmark (M1-11) to
/// `%LOCALAPPDATA%\Relay\benchmarks.json`. Empty until a benchmark has run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkResult {
    pub score: f64,
    pub latency_ms: u64,
}

pub fn benchmarks() -> BTreeMap<String, BenchmarkResult> {
    std::fs::read_to_string(crate::paths::data_dir().join("benchmarks.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(total_mb: u64) -> Gpu {
        Gpu {
            name: "Test GPU".into(),
            vram_total_mb: total_mb,
            vram_free_mb: total_mb,
            driver: "1".into(),
        }
    }

    #[test]
    fn catalog_is_complete() {
        let models = models();
        assert_eq!(models.len(), 5);
        for m in &models {
            assert!(
                m.quant(&m.default_quant).is_some(),
                "{} default quant",
                m.id
            );
            assert!(["grounding", "describe", "both"].contains(&m.role.as_str()));
            assert!(m.quants.iter().all(|q| q.size > 1_000_000_000));
            assert!(m.mmproj_size > 0);
        }
    }

    #[test]
    fn rtx_5080_gets_holo_3_1() {
        let rec = recommend(Some(&gpu(16_303)));
        let ids: Vec<_> = rec.picks.iter().map(|p| p.model_id.as_str()).collect();
        assert_eq!(ids, ["holo-3.1-9b"]);
    }

    #[test]
    fn smaller_gpus_fall_back() {
        assert_eq!(
            recommend(Some(&gpu(10_240))).picks[0].model_id,
            "holo-3.1-9b"
        );
        assert_eq!(recommend(Some(&gpu(9_216))).picks[0].model_id, "holo2-8b");
        assert_eq!(recommend(Some(&gpu(6_144))).picks[0].model_id, "holo2-4b");
        assert!(recommend(Some(&gpu(2_048))).picks.is_empty());
        assert!(recommend(None).picks.is_empty());
    }

    #[test]
    fn fit_badges() {
        let g = gpu(16_303);
        let eight = find("ui-venus-1.5-8b")
            .unwrap()
            .vram_bytes("Q4_K_M")
            .unwrap();
        let q8 = find("qwen3-vl-8b-instruct")
            .unwrap()
            .vram_bytes("Q8_0")
            .unwrap();
        assert_eq!(fit(Some(&g), eight, eight), Fit::Fits);
        assert_eq!(fit(Some(&g), q8, eight), Fit::Tight);
        assert_eq!(fit(Some(&gpu(4_096)), eight, 0), Fit::WontFit);
        assert_eq!(fit(None, eight, 0), Fit::NoGpu);
    }
}
