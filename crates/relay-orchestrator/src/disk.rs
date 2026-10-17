//! "What is using my disk space?": drive totals plus the biggest folders and files under some roots.
//! Only sizes and names are read, never file contents. The scan is bounded in time and entries.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

#[derive(Debug, Default)]
pub struct Scan {
    /// Size of each top-level entry under each root.
    pub folders: Vec<(PathBuf, u64)>,
    pub files: Vec<(PathBuf, u64)>,
    pub scanned: usize,
    pub complete: bool,
}

/// Walk `roots` and add up sizes, stopping after `budget` or `max_entries`.
pub fn scan(roots: &[PathBuf], keep: usize, budget: Duration, max_entries: usize) -> Scan {
    let start = Instant::now();
    let mut totals: HashMap<PathBuf, u64> = HashMap::new();
    let mut files: Vec<(PathBuf, u64)> = Vec::new();
    let mut scanned = 0usize;
    let mut complete = true;
    'roots: for root in roots {
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            scanned += 1;
            if scanned >= max_entries || start.elapsed() > budget {
                complete = false;
                break 'roots;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
            let top = rel
                .components()
                .next()
                .map(|c| root.join(c.as_os_str()))
                .unwrap_or_else(|| root.clone());
            *totals.entry(top).or_default() += size;
            files.push((entry.path().to_path_buf(), size));
            if files.len() > keep * 8 {
                files.sort_by_key(|f| std::cmp::Reverse(f.1));
                files.truncate(keep);
            }
        }
    }
    let mut folders: Vec<(PathBuf, u64)> = totals.into_iter().filter(|(_, s)| *s > 0).collect();
    folders.sort_by_key(|f| std::cmp::Reverse(f.1));
    folders.truncate(keep);
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    files.truncate(keep);
    Scan {
        folders,
        files,
        scanned,
        complete,
    }
}

/// Plain-text report for the owner.
pub fn report(drive: Option<(&Path, u64, u64)>, roots: &[PathBuf], s: &Scan) -> String {
    let mut out = String::new();
    if let Some((path, total, free)) = drive {
        let used = total.saturating_sub(free);
        let pct = (used * 100).checked_div(total).unwrap_or(0);
        out.push_str(&format!(
            "Drive {}: {} free of {} ({pct}% used).\n",
            path.display(),
            human(free),
            human(total)
        ));
    }
    let names: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
    out.push_str(&format!("\nLargest folders in {}:\n", names.join(", ")));
    if s.folders.is_empty() {
        out.push_str("- (nothing found)\n");
    }
    for (p, size) in &s.folders {
        out.push_str(&format!("- {} — {}\n", p.display(), human(*size)));
    }
    out.push_str("\nLargest files:\n");
    for (p, size) in &s.files {
        out.push_str(&format!("- {} — {}\n", p.display(), human(*size)));
    }
    if !s.complete {
        out.push_str(&format!(
            "\n(Stopped after looking at {} items to stay quick; there may be more.)\n",
            s.scanned
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_a_person_would_say_them() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KB");
        assert_eq!(human(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn the_biggest_folders_and_files_come_first() {
        let root = std::env::temp_dir().join(format!("relay-disk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::create_dir_all(root.join("big").join("deep"));
        let _ = std::fs::create_dir_all(root.join("small"));
        let _ = std::fs::write(
            root.join("big").join("deep").join("a.bin"),
            vec![0u8; 50_000],
        );
        let _ = std::fs::write(root.join("small").join("b.txt"), vec![0u8; 10]);
        let s = scan(
            std::slice::from_ref(&root),
            5,
            Duration::from_secs(5),
            10_000,
        );
        assert!(s.complete);
        assert_eq!(s.folders[0].0, root.join("big"));
        assert_eq!(s.folders[0].1, 50_000);
        assert!(s.files[0].0.ends_with("a.bin"));
        let text = report(
            Some((root.as_path(), 1000, 400)),
            std::slice::from_ref(&root),
            &s,
        );
        assert!(text.contains("400 B free of 1000 B (60% used)") && text.contains("48.8 KB"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
