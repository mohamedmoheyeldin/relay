# Build and release

## Prerequisites (Windows)
Rust stable, Node 24 (via fnm), pnpm 10+, WebView2 (present on Windows 11), `cargo-tauri` v2. Optional: Claude Code CLI
signed in, Codex CLI signed in. The local AI engine downloads itself on first run.

## Checks (must pass before a commit)
```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir ui install --frozen-lockfile
pnpm --dir ui typecheck; pnpm --dir ui test; pnpm --dir ui build
```

## Build
```powershell
cd src-tauri
cargo tauri build --no-bundle      # -> ..\target\release\relay-app.exe
```
That single executable is both the installer and the app (see D25 in `CLAUDE.md`): run from anywhere it shows the Relay
Setup window and installs to `C:\Program Files\Relay` through an elevated worker (one UAC prompt); run from the install
folder it is the app. Copy it as `Relay Setup.exe` for distribution.

Release builds omit the `e2e` feature, which is the only thing that lets WebView2 remote debugging through.

## Desktop E2E build
`cargo build --release --features e2e` then run the suite in `e2e/` (see Testing-Strategy.md).

## Release limits
Nothing is signed. Windows SmartScreen will warn on an unsigned download; code signing needs a certificate the owner
has not provided. macOS notarisation likewise needs an Apple developer account. No auto-update channel exists yet.
Commits, pushes, merges and releases happen only when the owner asks.
