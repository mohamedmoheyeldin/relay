# Local perception research (2026-10-14)

Target: RTX 5080 16 GB (Blackwell, sm_120), driver 617.42 / CUDA 13.4, ~13.5 GB free at idle. Model must stay in VRAM.

## GUI-grounding models that fit
See the table in [decisions.md D-005](../decisions.md#d-005-local-model-choice-for-a-16-gb-rtx-5080-vram-only). Published scores are on unquantized weights; M1-11 re-measures at Q4/Q8 on our screens.

Notes:
- Fine-tunes use their own prompt and coordinate conventions (absolute px vs 0-1000 relative). Each gets an adapter in `relay_screen/vlm/`.
- Grounding fine-tunes are weak at free-form description; keep a general VLM (Qwen3-VL-8B-Instruct) for `describe`/`verify`.
- 8B at Q8_0 ~ 9 GB with mmproj; Q4_K_M ~ 5-6 GB. Two 8B Q8 models do not fit together with KV cache.

## Runtimes
- **llama.cpp `llama-server`** (MIT): `-m model.gguf --mmproj mmproj.gguf -ngl 99 -fa`, OpenAI-compatible API; Qwen2.5-VL and Qwen3-VL supported; Windows CUDA 12.x/13.x builds. Loads any community GGUF + mmproj. Chosen (D-004).
- **Ollama** (MIT): `qwen3-vl` since v0.12.7; `keep_alive` per request (`0` unloads); embeddable standalone zip; custom fine-tune import less reliable. Optional backend.
- Blackwell needs CUDA >= 12.8 builds.

## Non-vision perception and input
- UIA: `uiautomation` (fast, thin) over `pywinauto` (heavier). Exact rects beat vision when available; Chromium/Electron trees can be shallow.
- OCR: Windows.Media.Ocr via `winocr` (built in, no GPU, word boxes; needs language pack). PaddleOCR optional later.
- OmniParser v2: detector AGPL-3.0, 39.6 on ScreenSpot-Pro. Skipped.
- Capture: `mss` (simple, multi-GPU); `dxcam` faster but per-adapter; Windows Graphics Capture for occluded windows.
- Input: SendInput via ctypes; UIPI blocks input to elevated windows from a non-elevated process.
- DPI: set PER_MONITOR_AWARE_V2 before any capture/UI import; normalize absolute moves to 0-65535 over the virtual desktop (`MOUSEEVENTF_VIRTUALDESK`).

## Packaging (M3)
- PyInstaller `--onedir` (not onefile) for the Python backend; Tauri 2 `bundle.externalBin` sidecars for backend and `llama-server`.
- Weights downloaded on first run with resume + SHA-256 into `%LOCALAPPDATA%\Relay\models`; quant picked from NVML free VRAM.

Sources: arXiv 2602.06391 (POINTS-GUI-G tables), arXiv 2602.09082 (UI-Venus-1.5), huggingface.co/Hcompany/Holo2-8B, huggingface.co/inclusionAI/UI-Venus-1.5-8B, ollama.com/library/qwen3-vl, docs.ollama.com/faq, docs.ollama.com/windows, github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md, github.com/ra1nty/DXcam, v2.tauri.app/develop/sidecar, huggingface.co/microsoft/OmniParser-v2.0.
