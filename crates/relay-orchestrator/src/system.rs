//! The real native execution layer: filesystem, process launch and UI automation through
//! `relay-os`. Inputs have already passed plan validation and the permission policy; these
//! functions still avoid shells and never build command lines from model text.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use relay_os::driver::Driver;
use relay_os::keys::parse_chord;
use relay_os::{Desktop, UiSelector};

use crate::context::DesktopFacts;
use crate::plan::{UiTarget, Verification};
use crate::services::{ServiceError, ServiceResult, Services};

const SEARCH_BUDGET: Duration = Duration::from_secs(8);

pub struct SystemServices {
    desktop: Arc<dyn Desktop>,
    driver: Arc<Driver>,
    /// Where file searches start when no root is given.
    search_roots: Vec<PathBuf>,
}

fn selector(t: &UiTarget) -> UiSelector {
    UiSelector {
        window: t.window.clone(),
        name: t.name.clone(),
        role: t.role.clone(),
        automation_id: t.automation_id.clone(),
    }
}

fn os_err(e: relay_os::OsError) -> ServiceError {
    match e {
        relay_os::OsError::Unsupported(what) => ServiceError::Unsupported(what.to_string()),
        relay_os::OsError::Stopped => ServiceError::Failed("stopped by the owner".into()),
        relay_os::OsError::InvalidArgument(m) => ServiceError::Transient(m),
        other => ServiceError::Failed(other.to_string()),
    }
}

fn io_err(what: &str, e: std::io::Error) -> ServiceError {
    ServiceError::Failed(format!("{what}: {e}"))
}

/// Match `name` against a query: plain text is a case-insensitive "contains"; `*` and `?` are wildcards.
pub fn name_matches(query: &str, name: &str) -> bool {
    let (q, n) = (query.to_lowercase(), name.to_lowercase());
    if !q.contains(['*', '?']) {
        return n.contains(&q);
    }
    fn glob(p: &[char], s: &[char]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some('*'), _) => glob(&p[1..], s) || (!s.is_empty() && glob(p, &s[1..])),
            (Some('?'), Some(_)) => glob(&p[1..], &s[1..]),
            (Some(a), Some(b)) if a == b => glob(&p[1..], &s[1..]),
            _ => false,
        }
    }
    glob(
        &q.chars().collect::<Vec<_>>(),
        &n.chars().collect::<Vec<_>>(),
    )
}

/// A bare program or app name: no separators, quotes or shell metacharacters.
pub fn is_plain_app_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty()
        && n.chars().count() <= 80
        && !n.contains([
            '\\', '/', ':', '"', '\'', '&', '|', '<', '>', '^', '%', ';', '`', '$', '(', ')',
        ])
}

fn start_menu_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(p) = std::env::var_os("ProgramData") {
        dirs.push(PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Some(p) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    dirs
}

/// The best Start menu shortcut for an app name: exact name first, otherwise the shortest containing it.
pub fn best_shortcut(name: &str, candidates: &[PathBuf]) -> Option<PathBuf> {
    let want = name.trim().to_lowercase();
    let stem = |p: &PathBuf| {
        p.file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    };
    candidates
        .iter()
        .filter(|p| stem(p).contains(&want))
        .min_by_key(|p| (stem(p) != want, stem(p).len()))
        .cloned()
}

fn find_shortcut(name: &str) -> Option<PathBuf> {
    let mut all = Vec::new();
    for dir in start_menu_dirs() {
        for entry in walkdir::WalkDir::new(dir)
            .max_depth(4)
            .into_iter()
            .flatten()
        {
            let p = entry.path();
            if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
                all.push(p.to_path_buf());
            }
        }
    }
    best_shortcut(name, &all)
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let exts: Vec<String> = if Path::new(name).extension().is_some() {
        vec![String::new()]
    } else {
        vec![".exe".into(), ".com".into()]
    };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|dir| {
        exts.iter().find_map(|ext| {
            let p = dir.join(format!("{name}{ext}"));
            p.is_file().then_some(p)
        })
    })
}

/// Look up a packaged (Store) app by name; returns its AppUserModelID.
async fn find_packaged_app(name: &str) -> Option<String> {
    // The name travels in an environment variable, never inside the script text.
    let script = "Get-StartApps | Where-Object { $_.Name -like ('*' + $env:RELAY_APP_NAME + '*') } | Select-Object -First 1 -ExpandProperty AppID";
    let mut cmd = tokio::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("RELAY_APP_NAME", name)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let out = tokio::time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .ok()?
        .ok()?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._!-+{}\\".contains(c)))
    .then_some(id)
}

impl SystemServices {
    pub fn new(desktop: Arc<dyn Desktop>, driver: Arc<Driver>, search_roots: Vec<PathBuf>) -> Self {
        Self {
            desktop,
            driver,
            search_roots,
        }
    }

    async fn blocking<T: Send + 'static>(
        f: impl FnOnce() -> ServiceResult<T> + Send + 'static,
    ) -> ServiceResult<T> {
        tokio::task::spawn_blocking(f)
            .await
            .map_err(|e| ServiceError::Failed(format!("internal error: {e}")))?
    }
    fn windows_matching(&self, title: &str) -> ServiceResult<Vec<relay_os::WindowInfo>> {
        let needle = title.to_lowercase();
        Ok(self
            .desktop
            .windows()
            .map_err(os_err)?
            .into_iter()
            .filter(|w| w.title.to_lowercase().contains(&needle))
            .collect())
    }
}

#[async_trait]
impl Services for SystemServices {
    async fn open_app(&self, name: &str) -> ServiceResult<String> {
        if !is_plain_app_name(name) {
            return Err(ServiceError::Failed(format!(
                "{name:?} is not a plain app name"
            )));
        }
        let name = name.trim().to_string();
        let n = name.clone();
        if let Some(lnk) = Self::blocking(move || Ok(find_shortcut(&n))).await? {
            opener::open(&lnk).map_err(|e| ServiceError::Failed(e.to_string()))?;
            return Ok(format!("Started {name}"));
        }
        let n = name.clone();
        if let Some(exe) = Self::blocking(move || Ok(find_on_path(&n))).await? {
            std::process::Command::new(&exe)
                .spawn()
                .map_err(|e| ServiceError::Failed(e.to_string()))?;
            return Ok(format!("Started {name}"));
        }
        if let Some(id) = find_packaged_app(&name).await {
            std::process::Command::new("explorer.exe")
                .arg(format!("shell:AppsFolder\\{id}"))
                .spawn()
                .map_err(|e| ServiceError::Failed(e.to_string()))?;
            return Ok(format!("Started {name}"));
        }
        Err(ServiceError::Failed(format!(
            "no installed app called “{name}” was found"
        )))
    }

    async fn open_url(&self, url: &str) -> ServiceResult<String> {
        let lower = url.trim().to_ascii_lowercase();
        if !(lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:"))
        {
            return Err(ServiceError::Failed(
                "only http, https and mailto links can be opened".into(),
            ));
        }
        if url.contains(['\n', '\r', '"', '|', '&', '<', '>', '^']) {
            return Err(ServiceError::Failed(
                "the link contains characters that are not allowed".into(),
            ));
        }
        opener::open_browser(url.trim()).map_err(|e| ServiceError::Failed(e.to_string()))?;
        Ok(format!("Opened {}", url.trim()))
    }

    async fn focus_window(&self, title: &str) -> ServiceResult<String> {
        let (d, t) = (self.desktop.clone(), title.to_string());
        Self::blocking(move || {
            d.focus_window(&t)
                .map(|w| format!("In front: {w}"))
                .map_err(|e| ServiceError::Transient(e.to_string()))
        })
        .await
    }

    async fn find_files(
        &self,
        query: &str,
        root: Option<&str>,
        max: usize,
    ) -> ServiceResult<Vec<String>> {
        let roots: Vec<PathBuf> = match root {
            Some(r) => vec![PathBuf::from(r)],
            None => self.search_roots.clone(),
        };
        let q = query.to_string();
        Self::blocking(move || {
            let started = Instant::now();
            let mut found = Vec::new();
            'roots: for root in roots {
                let walker = walkdir::WalkDir::new(&root)
                    .follow_links(false)
                    .into_iter()
                    .filter_entry(|e| {
                        let n = e.file_name().to_string_lossy();
                        !(n.starts_with('.') && e.depth() > 0)
                            && n != "node_modules"
                            && n != "$RECYCLE.BIN"
                    });
                for entry in walker.flatten() {
                    if started.elapsed() > SEARCH_BUDGET {
                        break 'roots;
                    }
                    if entry.depth() > 0 && name_matches(&q, &entry.file_name().to_string_lossy()) {
                        found.push(entry.path().display().to_string());
                        if found.len() >= max {
                            break 'roots;
                        }
                    }
                }
            }
            Ok(found)
        })
        .await
    }

    async fn list_dir(&self, path: &str) -> ServiceResult<Vec<String>> {
        let mut rd = tokio::fs::read_dir(path)
            .await
            .map_err(|e| io_err("cannot list", e))?;
        let mut out = Vec::new();
        while let Ok(Some(entry)) = rd.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.metadata().await.map(|m| m.is_dir()).unwrap_or(false);
            out.push(if is_dir { format!("{name}\\") } else { name });
            if out.len() >= 500 {
                break;
            }
        }
        out.sort();
        Ok(out)
    }

    async fn read_text(&self, path: &str, max_bytes: usize) -> ServiceResult<String> {
        use tokio::io::AsyncReadExt;
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|e| io_err("cannot open", e))?;
        let mut buf = Vec::new();
        file.take(max_bytes as u64)
            .read_to_end(&mut buf)
            .await
            .map_err(|e| io_err("cannot read", e))?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    async fn create_dir(&self, path: &str) -> ServiceResult<String> {
        tokio::fs::create_dir_all(path)
            .await
            .map_err(|e| io_err("cannot create the folder", e))?;
        Ok(format!("Created {path}"))
    }

    async fn move_path(&self, from: &str, to: &str) -> ServiceResult<String> {
        if tokio::fs::try_exists(to).await.unwrap_or(false) {
            return Err(ServiceError::Failed(format!(
                "{to} already exists; nothing was overwritten"
            )));
        }
        if let Some(parent) = Path::new(to).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| io_err("cannot create the target folder", e))?;
        }
        tokio::fs::rename(from, to)
            .await
            .map_err(|e| io_err("cannot move", e))?;
        Ok(format!("Moved {from} to {to}"))
    }

    async fn write_text(
        &self,
        path: &str,
        content: &str,
        overwrite: bool,
    ) -> ServiceResult<String> {
        if !overwrite && tokio::fs::try_exists(path).await.unwrap_or(false) {
            return Err(ServiceError::Failed(format!(
                "{path} already exists; overwrite was not requested"
            )));
        }
        if let Some(parent) = Path::new(path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| io_err("cannot create the folder", e))?;
        }
        tokio::fs::write(path, content)
            .await
            .map_err(|e| io_err("cannot write", e))?;
        Ok(format!("Wrote {} bytes to {path}", content.len()))
    }

    async fn move_to_trash(&self, paths: &[String]) -> ServiceResult<String> {
        let list: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let n = list.len();
        Self::blocking(move || {
            trash::delete_all(&list).map_err(|e| {
                ServiceError::Failed(format!("cannot move to the Recycle Bin: {e}"))
            })?;
            Ok(format!("Moved {n} item(s) to the Recycle Bin"))
        })
        .await
    }

    async fn ui_click(&self, target: &UiTarget) -> ServiceResult<String> {
        let (d, sel) = (self.desktop.clone(), selector(target));
        Self::blocking(move || {
            d.ui_invoke(&sel).map_err(|e| match e {
                relay_os::OsError::InvalidArgument(m) => ServiceError::Transient(m),
                other => os_err(other),
            })
        })
        .await
    }

    async fn ui_set_text(&self, target: &UiTarget, text: &str) -> ServiceResult<String> {
        let (d, sel, text) = (self.desktop.clone(), selector(target), text.to_string());
        Self::blocking(move || d.ui_set_value(&sel, &text).map_err(os_err)).await
    }

    async fn type_text(&self, text: &str) -> ServiceResult<String> {
        let (driver, text) = (self.driver.clone(), text.to_string());
        Self::blocking(move || {
            driver.type_text(&text).map_err(os_err)?;
            Ok(format!("Typed {} character(s)", text.chars().count()))
        })
        .await
    }

    async fn press_keys(&self, keys: &str) -> ServiceResult<String> {
        let (driver, keys) = (self.driver.clone(), keys.to_string());
        Self::blocking(move || {
            let chord = parse_chord(&keys).map_err(|e| ServiceError::Failed(e.to_string()))?;
            driver.key(&chord, 1).map_err(os_err)?;
            Ok(format!("Pressed {keys}"))
        })
        .await
    }

    async fn check(&self, condition: &Verification) -> ServiceResult<bool> {
        match condition {
            Verification::WindowExists { title } => Ok(!self.windows_matching(title)?.is_empty()),
            Verification::WindowActive { title } => {
                Ok(self.windows_matching(title)?.iter().any(|w| w.foreground))
            }
            Verification::FileExists { path } => {
                Ok(tokio::fs::try_exists(path).await.unwrap_or(false))
            }
            Verification::FileMissing { path } => {
                Ok(!tokio::fs::try_exists(path).await.unwrap_or(false))
            }
            Verification::UiElementExists { target } => {
                let (d, sel) = (self.desktop.clone(), selector(target));
                Self::blocking(move || {
                    Ok(d.ui_find(&sel, 1).map(|f| !f.is_empty()).unwrap_or(false))
                })
                .await
            }
            Verification::UiTextContains { target, text } => {
                let (d, sel, want) = (self.desktop.clone(), selector(target), text.to_lowercase());
                Self::blocking(move || {
                    Ok(d.ui_find(&sel, 3)
                        .map(|found| {
                            found.iter().any(|e| {
                                e.value
                                    .as_deref()
                                    .unwrap_or_default()
                                    .to_lowercase()
                                    .contains(&want)
                                    || e.name.to_lowercase().contains(&want)
                            })
                        })
                        .unwrap_or(false))
                })
                .await
            }
        }
    }

    async fn snapshot(&self) -> ServiceResult<DesktopFacts> {
        let d = self.desktop.clone();
        Self::blocking(move || {
            let windows = d.windows().map_err(os_err)?;
            let active = windows
                .iter()
                .find(|w| w.foreground)
                .map(|w| w.title.clone());
            let ui_outline = d.ui_outline(None, 80).ok().filter(|o| !o.is_empty());
            Ok(DesktopFacts {
                active_window: active,
                window_titles: windows.into_iter().map(|w| w.title).collect(),
                ui_outline,
                file_names: Vec::new(),
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_os::driver::{StopFlag, Timing};
    use relay_os::testing::FakeDesktop;
    use relay_os::{Rect, UiElement, WindowInfo};

    fn rig() -> (Arc<FakeDesktop>, SystemServices, PathBuf) {
        let fake = Arc::new(FakeDesktop::new());
        let driver = Arc::new(Driver::with_timing(
            fake.clone(),
            StopFlag::new(),
            Timing::instant(),
        ));
        let dir = std::env::temp_dir().join(format!(
            "relay-sys-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (
            fake.clone(),
            SystemServices::new(fake, driver, vec![dir.clone()]),
            dir,
        )
    }

    fn win(title: &str, fg: bool) -> WindowInfo {
        WindowInfo {
            title: title.into(),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            foreground: fg,
        }
    }

    #[test]
    fn wildcard_and_plain_matching() {
        assert!(name_matches("report", "Quarterly REPORT final.docx"));
        assert!(name_matches("*.zip", "a.ZIP"));
        assert!(!name_matches("*.zip", "a.zip.txt"));
        assert!(name_matches("a?c", "abc"));
        assert!(!name_matches("a?c", "abbc"));
        assert!(name_matches("*", "anything"));
    }

    #[test]
    fn app_names_cannot_smuggle_commands_or_paths() {
        for ok in ["Notepad", "Visual Studio Code", "Microsoft Edge"] {
            assert!(is_plain_app_name(ok), "{ok}");
        }
        for bad in [
            "",
            "C:\\Windows\\cmd.exe",
            "a & calc",
            "x|y",
            "..\\evil",
            "a;b",
            "$(calc)",
            "`x`",
            "https://x",
        ] {
            assert!(!is_plain_app_name(bad), "{bad}");
        }
    }

    #[test]
    fn shortcut_choice_prefers_exact_then_shortest() {
        let c = vec![
            PathBuf::from(r"C:\m\Notepad++ Plus.lnk"),
            PathBuf::from(r"C:\m\Notepad.lnk"),
            PathBuf::from(r"C:\m\Other.lnk"),
        ];
        assert_eq!(
            best_shortcut("notepad", &c).unwrap(),
            PathBuf::from(r"C:\m\Notepad.lnk")
        );
        assert_eq!(
            best_shortcut("plus", &c).unwrap(),
            PathBuf::from(r"C:\m\Notepad++ Plus.lnk")
        );
        assert!(best_shortcut("zzz", &c).is_none());
    }

    #[tokio::test]
    async fn file_operations_round_trip_and_never_overwrite_silently() {
        let (_, s, dir) = rig();
        let d = |p: &str| dir.join(p).display().to_string();
        s.create_dir(&d("sub")).await.unwrap();
        s.write_text(&d("sub/a.txt"), "hello", false).await.unwrap();
        assert!(
            s.write_text(&d("sub/a.txt"), "x", false).await.is_err(),
            "no silent overwrite"
        );
        s.write_text(&d("sub/a.txt"), "hello world", true)
            .await
            .unwrap();
        assert_eq!(
            s.read_text(&d("sub/a.txt"), 5).await.unwrap(),
            "hello",
            "reads are capped"
        );
        assert_eq!(s.list_dir(&d("sub")).await.unwrap(), vec!["a.txt"]);
        s.move_path(&d("sub/a.txt"), &d("moved/b.txt"))
            .await
            .unwrap();
        assert!(
            s.move_path(&d("moved/b.txt"), &d("moved/b.txt"))
                .await
                .is_err(),
            "target exists"
        );
        let found = s.find_files("*.txt", None, 10).await.unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("b.txt"));
        assert!(s
            .find_files("nothing-like-this", None, 10)
            .await
            .unwrap()
            .is_empty());
        assert!(s.read_text(&d("missing.txt"), 10).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn find_files_respects_the_result_limit() {
        let (_, s, dir) = rig();
        for i in 0..5 {
            std::fs::write(dir.join(format!("f{i}.log")), "x").unwrap();
        }
        assert_eq!(s.find_files("*.log", None, 3).await.unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn urls_are_filtered() {
        let (_, s, dir) = rig();
        for bad in [
            "file:///C:/Windows/System32/calc.exe",
            "javascript:1",
            "https://a.com\" & calc",
        ] {
            assert!(s.open_url(bad).await.is_err(), "{bad}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn windows_and_verification_use_the_live_state() {
        let (fake, s, dir) = rig();
        fake.set_windows(vec![
            win("Untitled - Notepad", true),
            win("Inbox - Mail", false),
        ]);
        assert!(s
            .check(&Verification::WindowExists {
                title: "notepad".into()
            })
            .await
            .unwrap());
        assert!(s
            .check(&Verification::WindowActive {
                title: "notepad".into()
            })
            .await
            .unwrap());
        assert!(!s
            .check(&Verification::WindowActive {
                title: "mail".into()
            })
            .await
            .unwrap());
        assert!(!s
            .check(&Verification::WindowExists {
                title: "calculator".into()
            })
            .await
            .unwrap());
        s.focus_window("mail").await.unwrap();
        assert!(s
            .check(&Verification::WindowActive {
                title: "mail".into()
            })
            .await
            .unwrap());
        let snap = s.snapshot().await.unwrap();
        assert_eq!(snap.active_window.as_deref(), Some("Inbox - Mail"));
        assert_eq!(snap.window_titles.len(), 2);
        let f = dir.join("exists.txt");
        std::fs::write(&f, "x").unwrap();
        assert!(s
            .check(&Verification::FileExists {
                path: f.display().to_string()
            })
            .await
            .unwrap());
        assert!(!s
            .check(&Verification::FileMissing {
                path: f.display().to_string()
            })
            .await
            .unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn ui_actions_resolve_targets_and_report_what_happened() {
        let (fake, s, dir) = rig();
        let rect = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        fake.set_elements(vec![
            UiElement {
                name: "Save".into(),
                role: "button".into(),
                automation_id: "btnSave".into(),
                rect,
                enabled: true,
                value: None,
            },
            UiElement {
                name: "File name".into(),
                role: "edit".into(),
                automation_id: "txtName".into(),
                rect,
                enabled: true,
                value: Some("draft".into()),
            },
            UiElement {
                name: "Delete".into(),
                role: "button".into(),
                automation_id: "".into(),
                rect,
                enabled: false,
                value: None,
            },
        ]);
        let t = |name: &str| UiTarget {
            name: Some(name.into()),
            ..UiTarget::default()
        };
        assert!(s.ui_click(&t("save")).await.unwrap().contains("Save"));
        assert!(
            s.ui_click(&t("Delete")).await.is_err(),
            "disabled controls are not clicked"
        );
        assert!(
            s.ui_click(&t("Nope")).await.unwrap_err().is_transient(),
            "a missing control may appear later"
        );
        s.ui_set_text(&t("File name"), "final").await.unwrap();
        assert!(s
            .check(&Verification::UiTextContains {
                target: t("File name"),
                text: "FINAL".into()
            })
            .await
            .unwrap());
        assert!(!s
            .check(&Verification::UiTextContains {
                target: t("File name"),
                text: "draft".into()
            })
            .await
            .unwrap());
        assert!(s
            .check(&Verification::UiElementExists { target: t("Save") })
            .await
            .unwrap());
        assert!(!s
            .check(&Verification::UiElementExists { target: t("Ghost") })
            .await
            .unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn keyboard_input_goes_through_the_driver() {
        let (fake, s, dir) = rig();
        s.type_text("hi").await.unwrap();
        s.press_keys("ctrl+s").await.unwrap();
        assert!(s.press_keys("ctrl+").await.is_err());
        assert!(!fake.events().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn unknown_apps_report_a_clear_error() {
        let (_, s, dir) = rig();
        let err = s.open_app("Definitely Not An App 12345").await.unwrap_err();
        assert!(err.to_string().contains("no installed app"), "{err}");
        assert!(s.open_app("a & calc").await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
