//! `eoka update`: replace this executable with a published release binary.

use crate::{cli::UpdateArgs, client, launch_spec::LaunchSpec, protocol, session};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const REPOSITORY: &str = "shrimp-software/eoka-tools";
const API_BASE: &str = "https://api.github.com";
const DOWNLOAD_BASE: &str = "https://github.com/shrimp-software/eoka-tools/releases/download";
const TAG_PREFIX: &str = "eoka-cli-v";
const MAX_ARCHIVE_BYTES: usize = 256 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;
const SKILL_HINT: &str = "npx skills add https://github.com/shrimp-software/eoka-tools/tree/main/crates/eoka-cli --skill eoka";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version(u64, u64, u64);

impl Version {
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        let version = Version(
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        );
        parts.next().is_none().then_some(version)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Release target published by `.github/workflows/release-eoka-cli.yml`.
fn release_target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") if cfg!(target_env = "gnu") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") if cfg!(target_env = "gnu") => Some("aarch64-unknown-linux-gnu"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        _ => None,
    }
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("eoka-cli/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|error| format!("Could not create the HTTP client: {error}"))
}

async fn get_bytes(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Request to {url} failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let rate_limited = matches!(status.as_u16(), 403 | 429);
        let hint = if rate_limited {
            "; GitHub may be rate limiting this address, try again later"
        } else {
            ""
        };
        return Err(format!("{url} returned {status}{hint}"));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Reading {url} failed: {error}"))?
    {
        if body.len() + chunk.len() > limit {
            return Err(format!("{url} exceeds the {limit} byte limit"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Highest published CLI version. The repository's "latest" release may be an
/// `eoka-server` tag, so the release list is filtered by tag prefix instead.
async fn latest_version(client: &reqwest::Client, api_base: &str) -> Result<Version, String> {
    let url = format!("{api_base}/repos/{REPOSITORY}/releases?per_page=100");
    let body = get_bytes(client, &url, MAX_METADATA_BYTES).await?;
    let releases: Vec<Value> = serde_json::from_slice(&body)
        .map_err(|error| format!("Could not parse the release list: {error}"))?;
    releases
        .iter()
        .filter(|release| {
            !release["draft"].as_bool().unwrap_or(false)
                && !release["prerelease"].as_bool().unwrap_or(false)
        })
        .filter_map(|release| release["tag_name"].as_str()?.strip_prefix(TAG_PREFIX))
        .filter_map(Version::parse)
        .max()
        .ok_or_else(|| "No published eoka-cli release was found".to_string())
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn expected_checksum(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let digest = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        (name == asset && digest.len() == 64).then(|| digest.to_ascii_lowercase())
    })
}

async fn download_verified(
    client: &reqwest::Client,
    download_base: &str,
    version: Version,
    target: &str,
) -> Result<Vec<u8>, String> {
    let asset = format!("eoka-cli-{target}.tar.gz");
    let release = format!("{download_base}/{TAG_PREFIX}{version}");
    let sums = get_bytes(client, &format!("{release}/SHA256SUMS"), MAX_METADATA_BYTES).await?;
    let expected = expected_checksum(&String::from_utf8_lossy(&sums), &asset)
        .ok_or_else(|| format!("SHA256SUMS for {version} has no entry for {asset}"))?;
    let archive = get_bytes(client, &format!("{release}/{asset}"), MAX_ARCHIVE_BYTES).await?;
    let actual = sha256_hex(&archive);
    if actual != expected {
        return Err(format!(
            "Checksum mismatch for {asset}: expected {expected}, got {actual}. Nothing was installed."
        ));
    }
    Ok(archive)
}

fn extract_binary(archive: &[u8]) -> Result<Vec<u8>, String> {
    let invalid = |error: std::io::Error| format!("Release archive is invalid: {error}");
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    let mut binary = None;
    for entry in tar.entries().map_err(invalid)? {
        let mut entry = entry.map_err(invalid)?;
        let path = entry.path().map_err(invalid)?.into_owned();
        if path != Path::new("eoka") || !entry.header().entry_type().is_file() || binary.is_some() {
            return Err(format!(
                "Release archive has an unexpected entry: {}",
                path.display()
            ));
        }
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(MAX_ARCHIVE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(invalid)?;
        if bytes.len() > MAX_ARCHIVE_BYTES {
            return Err("Release binary exceeds the size limit".to_string());
        }
        binary = Some(bytes);
    }
    binary.ok_or_else(|| "Release archive does not contain eoka".to_string())
}

fn probe_version(path: &Path) -> Result<String, String> {
    let mut attempts = 0;
    loop {
        match std::process::Command::new(path).arg("--version").output() {
            Ok(output) => return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string()),
            // A concurrent fork can briefly inherit the just-written file open
            // for writing, which makes exec fail with ETXTBSY.
            Err(error) if error.raw_os_error() == Some(libc::ETXTBSY) && attempts < 20 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => return Err(format!("Downloaded binary did not run: {error}")),
        }
    }
}

/// Write beside `dest`, confirm the new binary reports `version`, then rename
/// over `dest`. `dest` is untouched unless every step succeeds.
fn install(binary: &[u8], dest: &Path, version: Version) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let directory = dest
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", dest.display()))?;
    let staged = directory.join(format!(".eoka-update-{}", std::process::id()));
    let _ = std::fs::remove_file(&staged);
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&staged)
            .map_err(|error| format!("Cannot write to {}: {error}", directory.display()))?;
        file.write_all(binary)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Cannot write {}: {error}", staged.display()))?;
        drop(file);
        let reported = probe_version(&staged)?;
        let expected = format!("eoka {version}");
        if reported != expected {
            return Err(format!(
                "Downloaded binary reports '{reported}', expected '{expected}'. Nothing was installed."
            ));
        }
        std::fs::rename(&staged, dest)
            .map_err(|error| format!("Cannot replace {}: {error}", dest.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&staged);
    }
    result
}

/// The first `eoka` on `PATH`, when it is a different file from `dest`.
fn path_shadow(dest: &Path, path_var: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let first = std::env::split_paths(path_var?)
        .map(|directory| directory.join("eoka"))
        .find(|candidate| candidate.is_file())?;
    let same = std::fs::canonicalize(&first).is_ok_and(|resolved| resolved == dest);
    (!same).then_some(first)
}

/// Running daemons whose version differs from `installed`.
async fn stale_daemons(installed: &str, kill: bool) -> Vec<Value> {
    let mut stale = Vec::new();
    for name in session::list_sessions() {
        if !client::is_daemon_running(&name) {
            continue;
        }
        let hello =
            client::send_command(&name, protocol::Request::SessionHello, LaunchSpec::Reuse).await;
        let version = hello
            .ok()
            .filter(|response| response.ok)
            .and_then(|response| response.data)
            .and_then(|data| data["daemon_version"].as_str().map(str::to_string));
        if version.as_deref() == Some(installed) {
            continue;
        }
        let killed = kill && client::kill_daemon(&name).unwrap_or(false);
        stale.push(json!({"session": name, "daemon_version": version, "killed": killed}));
    }
    stale
}

fn summary(data: &Value) -> String {
    use std::fmt::Write;
    let current = data["current"].as_str().unwrap_or_default();
    let latest = data["latest"].as_str().unwrap_or_default();
    let path = data["path"].as_str().unwrap_or_default();
    let updated = data["updated"].as_bool().unwrap_or(false);
    let mut text = if updated && latest == current {
        format!("Reinstalled eoka {latest} ({path})")
    } else if updated {
        format!("Updated eoka {current} -> {latest} ({path})")
    } else if data["update_available"].as_bool().unwrap_or(false) {
        format!("eoka {latest} is available (installed: {current}). Run: eoka update")
    } else if latest == current {
        format!("eoka {current} is up to date")
    } else {
        format!("eoka {current} is newer than the latest release {latest}")
    };
    for daemon in data["stale_daemons"].as_array().into_iter().flatten() {
        let name = daemon["session"].as_str().unwrap_or_default();
        let version = daemon["daemon_version"].as_str().unwrap_or("unknown");
        if daemon["killed"].as_bool().unwrap_or(false) {
            let _ = write!(text, "\nStopped session '{name}' (daemon {version})");
        } else {
            let _ = write!(
                text,
                "\nSession '{name}' still runs daemon {version}; restart it with: eoka --session '{name}' kill"
            );
        }
    }
    if let Some(shadow) = data["path_shadowed_by"].as_str() {
        let _ = write!(text, "\nWarning: {shadow} comes first on PATH");
    }
    if updated {
        let _ = write!(text, "\nRefresh the agent skill with: {SKILL_HINT}");
    }
    text
}

async fn update(args: &UpdateArgs) -> Result<Value, String> {
    let dest = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|error| format!("Cannot locate the running eoka binary: {error}"))?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .ok_or_else(|| "This build has an unrecognized version".to_string())?;
    let client = http_client()?;
    let latest = latest_version(&client, API_BASE).await?;
    let update_available = latest > current;
    let target = release_target();
    let updated = !args.check && (update_available || args.force);
    if updated {
        let target = target.ok_or_else(|| {
            format!(
                "No release binary is published for {}-{}. Run: cargo install eoka-cli --locked",
                std::env::consts::ARCH,
                std::env::consts::OS
            )
        })?;
        let archive = download_verified(&client, DOWNLOAD_BASE, latest, target).await?;
        install(&extract_binary(&archive)?, &dest, latest)?;
    }
    let installed = if updated { latest } else { current };
    let stale = stale_daemons(&installed.to_string(), args.kill_daemons).await;
    let shadow = path_shadow(&dest, std::env::var_os("PATH").as_deref());
    let mut data = json!({
        "current": current.to_string(),
        "latest": latest.to_string(),
        "update_available": update_available,
        "updated": updated,
        "path": dest.to_string_lossy(),
        "target": target,
        "stale_daemons": stale,
        "path_shadowed_by": shadow.map(|path| path.to_string_lossy().to_string()),
    });
    data["text"] = json!(summary(&data));
    Ok(data)
}

pub(crate) async fn run(args: &UpdateArgs) -> protocol::Response {
    match update(args).await {
        Ok(data) => protocol::Response::ok(data),
        Err(error) => protocol::Response::err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const TARGET: &str = "x86_64-unknown-linux-gnu";

    async fn serve(routes: HashMap<String, Vec<u8>>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let routes = routes.clone();
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 1024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        match stream.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => request.extend_from_slice(&buffer[..read]),
                        }
                    }
                    let request = String::from_utf8_lossy(&request).to_string();
                    let path = request.split_whitespace().nth(1).unwrap_or_default();
                    let path = path.split('?').next().unwrap_or_default();
                    let (status, body) = match routes.get(path) {
                        Some(body) => ("200 OK", body.clone()),
                        None => ("404 Not Found", Vec::new()),
                    };
                    let head = format!(
                        "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&body).await;
                });
            }
        });
        format!("http://{address}")
    }

    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (name, bytes) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *bytes).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn script(version: &str) -> Vec<u8> {
        format!("#!/bin/sh\necho 'eoka {version}'\n").into_bytes()
    }

    fn release_routes(version: &str, archive: &[u8], sums: String) -> HashMap<String, Vec<u8>> {
        let asset = format!("eoka-cli-{TARGET}.tar.gz");
        HashMap::from([
            (format!("/eoka-cli-v{version}/{asset}"), archive.to_vec()),
            (
                format!("/eoka-cli-v{version}/SHA256SUMS"),
                sums.into_bytes(),
            ),
        ])
    }

    #[test]
    fn versions_parse_and_order_numerically() {
        assert_eq!(Version::parse("0.10.3"), Some(Version(0, 10, 3)));
        assert!(Version(0, 10, 0) > Version(0, 9, 9));
        for invalid in ["0.10", "0.10.3.1", "0.10.x", "v0.10.3", "0.10.3-rc1"] {
            assert_eq!(Version::parse(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn this_host_has_a_published_target() {
        assert!(release_target().is_some_and(|target| target.contains(std::env::consts::ARCH)));
    }

    #[tokio::test]
    async fn latest_version_ignores_other_tags_drafts_and_prereleases() {
        let releases = json!([
            {"tag_name": "eoka-server-v9.0.0", "draft": false, "prerelease": false},
            {"tag_name": "eoka-cli-v0.9.1", "draft": false, "prerelease": false},
            {"tag_name": "eoka-cli-v0.10.3", "draft": false, "prerelease": false},
            {"tag_name": "eoka-cli-v0.11.0", "draft": true, "prerelease": false},
            {"tag_name": "eoka-cli-v0.12.0", "draft": false, "prerelease": true},
            {"tag_name": "eoka-cli-vnext", "draft": false, "prerelease": false},
        ]);
        let base = serve(HashMap::from([(
            format!("/repos/{REPOSITORY}/releases"),
            releases.to_string().into_bytes(),
        )]))
        .await;
        let latest = latest_version(&http_client().unwrap(), &base).await;
        assert_eq!(latest, Ok(Version(0, 10, 3)));
    }

    #[tokio::test]
    async fn latest_version_reports_a_missing_release_list() {
        let base = serve(HashMap::new()).await;
        let error = latest_version(&http_client().unwrap(), &base)
            .await
            .unwrap_err();
        assert!(error.contains("404"), "{error}");
    }

    #[tokio::test]
    async fn download_requires_a_matching_checksum() {
        let tarball = archive(&[("eoka", &script("1.2.3"))]);
        let asset = format!("eoka-cli-{TARGET}.tar.gz");
        let client = http_client().unwrap();
        let version = Version(1, 2, 3);

        let good = format!("{}  {asset}\n", sha256_hex(&tarball));
        let base = serve(release_routes("1.2.3", &tarball, good)).await;
        let fetched = download_verified(&client, &base, version, TARGET).await;
        assert_eq!(fetched, Ok(tarball.clone()));

        let wrong = format!("{}  {asset}\n", sha256_hex(b"something else"));
        let base = serve(release_routes("1.2.3", &tarball, wrong)).await;
        let error = download_verified(&client, &base, version, TARGET)
            .await
            .unwrap_err();
        assert!(error.contains("Checksum mismatch"), "{error}");

        let other = format!("{}  eoka-cli-other.tar.gz\n", sha256_hex(&tarball));
        let base = serve(release_routes("1.2.3", &tarball, other)).await;
        let error = download_verified(&client, &base, version, TARGET)
            .await
            .unwrap_err();
        assert!(error.contains("no entry"), "{error}");
    }

    #[test]
    fn extract_accepts_only_a_single_eoka_file() {
        let binary = script("1.2.3");
        assert_eq!(
            extract_binary(&archive(&[("eoka", &binary)])),
            Ok(binary.clone())
        );
        for entries in [
            vec![("eoka-evil", binary.as_slice())],
            vec![("eoka", binary.as_slice()), ("eoka", binary.as_slice())],
            vec![("eoka", binary.as_slice()), ("extra", b"x".as_slice())],
            vec![("bin/eoka", binary.as_slice())],
        ] {
            let error = extract_binary(&archive(&entries)).unwrap_err();
            assert!(error.contains("unexpected entry"), "{error}");
        }
        assert!(extract_binary(&archive(&[])).is_err());
        assert!(extract_binary(b"not a tarball").is_err());
    }

    #[test]
    fn install_replaces_only_a_binary_reporting_the_expected_version() {
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("eoka");
        std::fs::write(&dest, "old").unwrap();

        let error = install(&script("9.9.9"), &dest, Version(1, 2, 3)).unwrap_err();
        assert!(error.contains("reports 'eoka 9.9.9'"), "{error}");
        assert_eq!(std::fs::read(&dest).unwrap(), b"old");

        install(&script("1.2.3"), &dest, Version(1, 2, 3)).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), script("1.2.3"));
        assert_eq!(probe_version(&dest).unwrap(), "eoka 1.2.3");
        let names: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["eoka"], "staging file must not be left behind");
    }

    #[test]
    fn path_shadow_reports_an_earlier_eoka() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let dest = std::fs::canonicalize(second.path()).unwrap().join("eoka");
        std::fs::write(&dest, "new").unwrap();
        let path = std::env::join_paths([first.path(), second.path()]).unwrap();
        assert_eq!(path_shadow(&dest, Some(&path)), None);
        std::fs::write(first.path().join("eoka"), "old").unwrap();
        assert_eq!(
            path_shadow(&dest, Some(&path)),
            Some(first.path().join("eoka"))
        );
        assert_eq!(path_shadow(&dest, None), None);
    }

    #[test]
    fn summary_names_the_next_step() {
        let mut data = json!({"current": "0.9.0", "latest": "0.10.3", "update_available": true, "updated": false, "stale_daemons": [], "path_shadowed_by": null});
        assert!(summary(&data).contains("Run: eoka update"));
        data["updated"] = json!(true);
        data["path"] = json!("/bin/eoka");
        data["stale_daemons"] =
            json!([{"session": "work", "daemon_version": "0.9.0", "killed": false}]);
        let text = summary(&data);
        assert!(text.contains("Updated eoka 0.9.0 -> 0.10.3"), "{text}");
        assert!(text.contains("eoka --session 'work' kill"), "{text}");
    }
}
