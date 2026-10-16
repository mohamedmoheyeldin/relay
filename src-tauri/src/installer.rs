//! Self-installing: the downloaded `Relay Setup.exe` is the app itself. Outside its install
//! folder it shows a Relay-themed setup window, copies itself to a per-user folder, adds
//! shortcuts and an uninstall entry, then starts the installed copy. `--uninstall` reverses it.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

pub const APP_NAME: &str = "Relay";
pub const INSTALLED_EXE: &str = "Relay.exe";
const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Relay";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Normal app.
    App,
    /// First run outside the install folder: offer to install.
    Install,
    /// `--uninstall`: offer to remove.
    Uninstall,
}

/// Where Relay installs itself (per user, no administrator rights needed).
pub fn install_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Programs")
        .join(APP_NAME)
}

/// Decide what this launch is for. Debug builds and `--run` always start the app.
pub fn detect_mode(args: &[String], current_exe: &Path, install: &Path, debug: bool) -> Mode {
    if args.iter().any(|a| a == "--uninstall") {
        return Mode::Uninstall;
    }
    if debug || args.iter().any(|a| a == "--run") {
        return Mode::App;
    }
    let inside = current_exe
        .parent()
        .map(|p| p.eq_ignore_ascii_case_path(install))
        .unwrap_or(false);
    if inside {
        Mode::App
    } else {
        Mode::Install
    }
}

trait PathCompare {
    fn eq_ignore_ascii_case_path(&self, other: &Path) -> bool;
}

impl PathCompare for Path {
    fn eq_ignore_ascii_case_path(&self, other: &Path) -> bool {
        self.to_string_lossy()
            .trim_end_matches('\\')
            .eq_ignore_ascii_case(other.to_string_lossy().trim_end_matches('\\'))
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct InstallOptions {
    pub desktop_shortcut: bool,
    pub start_with_windows: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallInfo {
    pub mode: Mode,
    pub version: String,
    pub install_dir: String,
    pub installed_version: Option<String>,
    pub data_dir: String,
    /// Size of the downloaded local AI (engine + models) in bytes, for the uninstall screen.
    pub data_bytes: u64,
}

/// Total size of a folder tree (0 if missing).
pub fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

fn hidden(program: &str) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn run(cmd: &mut Command) -> Result<()> {
    let out = cmd.output().context("starting a helper command")?;
    if out.status.success() {
        Ok(())
    } else {
        Err(anyhow!("{}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// PowerShell that creates a shortcut.
pub fn shortcut_script(link: &Path, target: &Path, workdir: &Path) -> String {
    let q = |p: &Path| p.display().to_string().replace('\'', "''");
    format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.IconLocation='{},0';$s.Description='Relay';$s.Save()",
        q(link),
        q(target),
        q(workdir),
        q(target)
    )
}

fn start_menu_link() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(r"Microsoft\Windows\Start Menu\Programs")
        .join(format!("{APP_NAME}.lnk"))
}

fn desktop_link() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Desktop")
        .join(format!("{APP_NAME}.lnk"))
}

fn make_shortcut(link: &Path, target: &Path, workdir: &Path) -> Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    run(hidden("powershell.exe").args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &shortcut_script(link, target, workdir),
    ]))
}

fn reg_add(key: &str, name: &str, kind: &str, data: &str) -> Result<()> {
    run(hidden("reg").args(["add", key, "/v", name, "/t", kind, "/d", data, "/f"]))
}

/// Stop a running installed copy so its file can be replaced.
fn stop_running_copy() {
    let _ = hidden("taskkill")
        .args(["/IM", INSTALLED_EXE, "/F", "/T"])
        .output();
    std::thread::sleep(std::time::Duration::from_millis(600));
}

/// Copy this program into `dir` as `Relay.exe`.
pub fn copy_self(source: &Path, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let target = dir.join(INSTALLED_EXE);
    std::fs::copy(source, &target).with_context(|| format!("copying to {}", target.display()))?;
    Ok(target)
}

/// Install for the current user. `on_step` receives (label, percent).
pub fn install(
    opts: InstallOptions,
    version: &str,
    mut on_step: impl FnMut(&str, u8),
) -> Result<PathBuf> {
    let source = std::env::current_exe().context("finding this program")?;
    let dir = install_dir();
    on_step("Closing any running copy", 10);
    stop_running_copy();
    on_step("Copying Relay", 30);
    let exe = copy_self(&source, &dir)?;
    on_step("Adding Relay to the Start menu", 55);
    make_shortcut(&start_menu_link(), &exe, &dir)?;
    if opts.desktop_shortcut {
        on_step("Adding a desktop shortcut", 65);
        make_shortcut(&desktop_link(), &exe, &dir)?;
    }
    on_step("Registering the uninstaller", 80);
    let exe_s = exe.display().to_string();
    reg_add(UNINSTALL_KEY, "DisplayName", "REG_SZ", APP_NAME)?;
    reg_add(UNINSTALL_KEY, "DisplayVersion", "REG_SZ", version)?;
    reg_add(UNINSTALL_KEY, "Publisher", "REG_SZ", APP_NAME)?;
    reg_add(
        UNINSTALL_KEY,
        "InstallLocation",
        "REG_SZ",
        &dir.display().to_string(),
    )?;
    reg_add(UNINSTALL_KEY, "DisplayIcon", "REG_SZ", &exe_s)?;
    reg_add(
        UNINSTALL_KEY,
        "UninstallString",
        "REG_SZ",
        &format!("\"{exe_s}\" --uninstall"),
    )?;
    reg_add(UNINSTALL_KEY, "NoModify", "REG_DWORD", "1")?;
    reg_add(UNINSTALL_KEY, "NoRepair", "REG_DWORD", "1")?;
    if opts.start_with_windows {
        reg_add(
            RUN_KEY,
            APP_NAME,
            "REG_SZ",
            &format!("\"{exe_s}\" --minimized"),
        )?;
    }
    on_step("Done", 100);
    Ok(exe)
}

/// Version of an existing install, if any.
pub fn installed_version() -> Option<String> {
    let out = hidden("reg")
        .args(["query", UNINSTALL_KEY, "/v", "DisplayVersion"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.contains("DisplayVersion"))
        .and_then(|l| l.split_whitespace().last())
        .map(str::to_string)
}

/// Remove shortcuts and registry entries, optionally the user's data, then schedule the
/// install folder's deletion (this program is running from it).
pub fn uninstall(
    delete_data: bool,
    data_dir: &Path,
    mut on_step: impl FnMut(&str, u8),
) -> Result<()> {
    on_step("Removing shortcuts", 20);
    let _ = std::fs::remove_file(start_menu_link());
    let _ = std::fs::remove_file(desktop_link());
    on_step("Removing registry entries", 45);
    let _ = hidden("reg").args(["delete", UNINSTALL_KEY, "/f"]).output();
    let _ = hidden("reg")
        .args(["delete", RUN_KEY, "/v", APP_NAME, "/f"])
        .output();
    if delete_data {
        on_step("Deleting your chats, settings and local AI", 65);
        let _ = std::fs::remove_dir_all(data_dir);
    }
    on_step("Removing Relay", 85);
    let dir = install_dir();
    // A short-lived helper deletes the folder once this process has exited.
    let cleanup = format!(
        "ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"",
        dir.display()
    );
    let mut cmd = hidden("cmd");
    cmd.args(["/C", &cleanup]);
    cmd.spawn().context("scheduling cleanup")?;
    on_step("Done", 100);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn mode_detection() {
        let install = Path::new(r"C:\Users\me\AppData\Local\Programs\Relay");
        let elsewhere = Path::new(r"C:\Users\me\Downloads\Relay Setup.exe");
        let inside = install.join("Relay.exe");
        assert_eq!(
            detect_mode(&args(&[]), elsewhere, install, false),
            Mode::Install
        );
        assert_eq!(detect_mode(&args(&[]), &inside, install, false), Mode::App);
        assert_eq!(
            detect_mode(
                &args(&[]),
                Path::new(r"c:\users\ME\appdata\local\programs\relay\Relay.exe"),
                install,
                false
            ),
            Mode::App,
            "paths compare case-insensitively"
        );
        assert_eq!(
            detect_mode(&args(&["--run"]), elsewhere, install, false),
            Mode::App
        );
        assert_eq!(detect_mode(&args(&[]), elsewhere, install, true), Mode::App);
        assert_eq!(
            detect_mode(&args(&["--uninstall"]), &inside, install, true),
            Mode::Uninstall
        );
    }

    #[test]
    fn shortcut_script_quotes_paths() {
        let s = shortcut_script(
            Path::new(r"C:\Users\O'Neil\Desktop\Relay.lnk"),
            Path::new(r"C:\Apps\Relay.exe"),
            Path::new(r"C:\Apps"),
        );
        assert!(s.contains("O''Neil"));
        assert!(s.contains("TargetPath='C:\\Apps\\Relay.exe'"));
    }

    #[test]
    fn copying_and_sizing() {
        let base = std::env::temp_dir().join(format!("relay-inst-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("Setup.exe");
        std::fs::write(&src, vec![1u8; 4096]).unwrap();
        let exe = copy_self(&src, &base.join("Programs").join("Relay")).unwrap();
        assert_eq!(exe.file_name().unwrap(), INSTALLED_EXE);
        assert_eq!(std::fs::read(&exe).unwrap().len(), 4096);
        // Re-installing over an existing copy replaces it.
        std::fs::write(&src, vec![2u8; 100]).unwrap();
        copy_self(&src, &base.join("Programs").join("Relay")).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap().len(), 100);
        assert_eq!(dir_size(&base.join("Programs")), 100);
        assert_eq!(dir_size(&base.join("missing")), 0);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn install_dir_is_per_user() {
        let d = install_dir();
        assert!(d.ends_with(r"Programs\Relay"));
    }
}
