//! Resumable downloads with SHA-256 checks (D-046).
//!
//! A file downloads to `<dest>.part`. If that exists, the download continues from its
//! length with an HTTP Range request. The whole file is hashed when it is complete,
//! and it is renamed into place only when the hash matches.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Shared flag the UI sets to stop a download; the .part file is kept for resuming.
pub type Cancel = Arc<AtomicBool>;

/// Run `work` until it ends or `cancel` is set. On cancel the future is dropped,
/// which stops any child process it started (they are spawned with kill_on_drop).
pub async fn until_cancelled<T>(
    work: impl std::future::Future<Output = anyhow::Result<T>>,
    cancel: &Cancel,
) -> anyhow::Result<T> {
    tokio::pin!(work);
    loop {
        tokio::select! {
            out = &mut work => return out,
            _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    anyhow::bail!("Cancelled.");
                }
            }
        }
    }
}

pub fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

pub async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("Relay/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// Download `url` to `dest`. `expected_sha256` may be empty to accept any content
/// (custom models whose hash Hugging Face does not report). Returns the file's hash.
pub async fn download(
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    expected_size: Option<u64>,
    cancel: &Cancel,
    on_progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<String> {
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    if dest.is_file() && !expected_sha256.is_empty() && sha256_file(dest).await? == expected_sha256
    {
        let len = dest.metadata()?.len();
        on_progress(len, len);
        return Ok(expected_sha256.to_string());
    }
    let part = part_path(dest);
    let mut have = tokio::fs::metadata(&part)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if expected_size.is_some_and(|size| have > size) {
        tokio::fs::remove_file(&part).await?;
        have = 0;
    }
    let client = client()?;
    let mut request = client.get(url);
    if have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("could not reach {url}"))?;
    let status = response.status();
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // The .part file is already complete.
    } else if !status.is_success() {
        bail!("download failed: HTTP {status} for {url}");
    } else {
        let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
        if !resumed {
            have = 0;
        }
        let total = response
            .content_length()
            .map(|len| len + have)
            .or(expected_size)
            .unwrap_or(0);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&part)
            .await?;
        let mut stream = response.bytes_stream();
        let mut done = have;
        let mut last_report = 0u64;
        on_progress(done, total);
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                file.flush().await?;
                bail!("cancelled");
            }
            let chunk = chunk.context("the download was interrupted")?;
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            if done - last_report >= 4 * 1024 * 1024 {
                on_progress(done, total);
                last_report = done;
            }
        }
        file.flush().await?;
        on_progress(done, total);
    }
    let actual = sha256_file(&part).await?;
    if !expected_sha256.is_empty() && actual != expected_sha256 {
        tokio::fs::remove_file(&part).await.ok();
        bail!("the downloaded file failed its SHA-256 check and was deleted; try again");
    }
    tokio::fs::rename(&part, dest).await?;
    Ok(actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_name() {
        assert_eq!(
            part_path(Path::new("/m/model.gguf")),
            PathBuf::from("/m/model.gguf.part")
        );
    }

    #[tokio::test]
    async fn hashes_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).await.unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn verified_file_is_not_downloaded_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, b"abc").unwrap();
        let cancel = Cancel::default();
        let sha = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        // The URL is never fetched because the file on disk already matches.
        let got = download(
            "http://127.0.0.1:9/never",
            &path,
            sha,
            Some(3),
            &cancel,
            &|_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(got, sha);
    }
}
