//! First-run and on-demand setup of Relay's built-in local AI: engine, server and model.

use std::sync::{Arc, Mutex};

use relay_agent::local::engine::{
    detect_gpu, recommended_model, EngineError, EngineManager, GpuInfo, SetupState, Step,
};
use relay_agent::LocalProvider;
use relay_protocol::UiEvent;
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::app::emit_event;

pub const PROGRESS_ID: &str = "local-ai";

pub struct LocalSetup {
    app: AppHandle,
    engine: Arc<EngineManager>,
    local: Arc<LocalProvider>,
    cancel: Mutex<Option<CancellationToken>>,
    gpu: Mutex<Option<GpuInfo>>,
}

impl LocalSetup {
    pub fn new(app: AppHandle, engine: Arc<EngineManager>, local: Arc<LocalProvider>) -> Arc<Self> {
        Arc::new(Self {
            app,
            engine,
            local,
            cancel: Mutex::new(None),
            gpu: Mutex::new(None),
        })
    }

    pub fn engine(&self) -> &Arc<EngineManager> {
        &self.engine
    }

    pub fn gpu(&self) -> Option<GpuInfo> {
        self.gpu.lock().ok().and_then(|g| g.clone())
    }

    /// The graphics card, detected once and remembered.
    pub async fn gpu_or_detect(&self) -> Option<GpuInfo> {
        if let Some(gpu) = self.gpu() {
            return Some(gpu);
        }
        let found = detect_gpu().await;
        if let Ok(mut slot) = self.gpu.lock() {
            *slot = found.clone();
        }
        found
    }

    pub fn local_client(&self) -> relay_agent::local::OllamaClient {
        self.local.client()
    }

    fn report(&self, stage: &str, percent: Option<u8>) {
        self.engine.set_state(SetupState {
            state: "running".into(),
            stage: stage.to_string(),
            percent,
            error: None,
        });
        emit_event(
            &self.app,
            &UiEvent::Progress {
                id: PROGRESS_ID.into(),
                label: stage.to_string(),
                percent,
                done: false,
                error: None,
            },
        );
    }

    fn finish(&self, error: Option<String>) {
        let ready = error.is_none();
        self.engine.set_state(SetupState {
            state: if ready { "ready" } else { "failed" }.into(),
            stage: String::new(),
            percent: ready.then_some(100),
            error: error.clone(),
        });
        emit_event(
            &self.app,
            &UiEvent::Progress {
                id: PROGRESS_ID.into(),
                label: if ready {
                    "Local AI is ready".into()
                } else {
                    "Local AI setup stopped".into()
                },
                percent: ready.then_some(100),
                done: true,
                error,
            },
        );
    }

    /// Start setup in the background (no-op if it is already running).
    pub fn start(self: &Arc<Self>) {
        let token = CancellationToken::new();
        {
            let Ok(mut slot) = self.cancel.lock() else {
                return;
            };
            if slot.is_some() {
                return;
            }
            *slot = Some(token.clone());
        }
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = this.run(&token).await;
            if let Ok(mut slot) = this.cancel.lock() {
                *slot = None;
            }
            match result {
                Ok(()) => this.finish(None),
                Err(EngineError::Cancelled) => {
                    this.engine.set_state(SetupState::idle());
                    this.finish(Some(
                        "Setup was cancelled. You can resume it from Settings.".into(),
                    ));
                }
                Err(e) => {
                    warn!("local AI setup failed: {e}");
                    this.finish(Some(e.to_string()));
                }
            }
        });
    }

    pub fn cancel(&self) {
        if let Some(token) = self.cancel.lock().ok().and_then(|c| c.clone()) {
            token.cancel();
        }
    }

    async fn run(&self, cancel: &CancellationToken) -> Result<(), EngineError> {
        self.report("Checking your computer", None);
        let gpu = detect_gpu().await;
        if let Ok(mut slot) = self.gpu.lock() {
            *slot = gpu.clone();
        }

        if !self.engine.is_installed() {
            let app = self.app.clone();
            let engine = self.engine.clone();
            let mut on_step = move |step: Step| {
                engine.set_state(SetupState {
                    state: "running".into(),
                    stage: step.label.clone(),
                    percent: step.percent,
                    error: None,
                });
                emit_event(
                    &app,
                    &UiEvent::Progress {
                        id: PROGRESS_ID.into(),
                        label: step.label,
                        percent: step.percent,
                        done: false,
                        error: None,
                    },
                );
            };
            self.engine.install(&mut on_step, cancel).await?;
        }

        self.report("Starting the local AI", None);
        self.engine.start().await?;

        let client = self.local.client();
        let models = client
            .models()
            .await
            .map_err(|e| EngineError::Start(e.to_string()))?;
        if !models.iter().any(|m| m.tools) {
            let model = recommended_model(gpu.as_ref());
            info!("downloading local model {model}");
            let app = self.app.clone();
            let engine = self.engine.clone();
            let name = model.to_string();
            let pulled = client.pull(model, move |p| {
                let label = format!("Downloading the AI model {name}");
                let percent = p.percent();
                engine.set_state(SetupState {
                    state: "running".into(),
                    stage: label.clone(),
                    percent,
                    error: None,
                });
                emit_event(
                    &app,
                    &UiEvent::Progress {
                        id: PROGRESS_ID.into(),
                        label,
                        percent,
                        done: false,
                        error: None,
                    },
                );
            });
            tokio::select! {
                _ = cancel.cancelled() => return Err(EngineError::Cancelled),
                r = pulled => r.map_err(|e| EngineError::Network(e.to_string()))?,
            }
        }
        Ok(())
    }
}
