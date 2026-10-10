//! Download the official vanilla server at test time and run one scenario against it.
//!
//! The jar is cached under `target/compat-cache` or `PUMPKIN_COMPAT_CACHE`. It is
//! never written into the git tree. Running the server writes `eula=true` into
//! that cache's world directory only.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::{Duration, Instant};

use pumpkin_data::packet::CURRENT_MC_VERSION;
use sha1::{Digest, Sha1};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::{Child, ChildStdin, Command};
use tokio::time::timeout;

use crate::error::Error;

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const READY_TIMEOUT: Duration = Duration::from_secs(300);

/// A vanilla dedicated server started from a jar downloaded for this run.
pub struct VanillaProcess {
    child: Child,
    stdin: ChildStdin,
    addr: SocketAddr,
    log: Vec<String>,
    _dir: TempDir,
}

impl VanillaProcess {
    /// Download the server jar for Pumpkin's current Minecraft version if needed, then start it.
    pub async fn start() -> Result<Self, Error> {
        ensure_java25()?;
        let jar = ensure_server_jar()?;
        let dir = TempDir::new().map_err(|err| Error::io("create vanilla world dir", err))?;
        let port = free_port().await?;
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        write_world_files(dir.path(), port)?;

        let mut command = java_command();
        command
            .current_dir(dir.path())
            .arg("-Xmx1G")
            .arg("-jar")
            .arg(&jar)
            .arg("nogui")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|err| Error::io("spawn vanilla server", err))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::vanilla("vanilla server was spawned without stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::vanilla("vanilla server was spawned without stdout"))?;
        let stderr = child.stderr.take();

        let mut log = Vec::new();
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(_)) = lines.next_line().await {}
            });
        }
        let mut lines = BufReader::new(stdout).lines();
        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::vanilla(format!(
                    "vanilla server did not finish starting\n{}",
                    log.join("\n")
                )));
            }
            match timeout(remaining, lines.next_line()).await {
                Ok(Ok(Some(line))) => {
                    let done = line.contains("Done (");
                    let eula = line.contains("You need to agree to the EULA");
                    push_log(&mut log, line);
                    if eula {
                        return Err(Error::vanilla(
                            "vanilla server refused to start because eula.txt was not accepted",
                        ));
                    }
                    if done {
                        break;
                    }
                }
                Ok(Ok(None)) | Err(_) => {
                    return Err(Error::vanilla(format!(
                        "vanilla server exited before it was ready\n{}",
                        log.join("\n")
                    )));
                }
                Ok(Err(err)) => return Err(Error::io("read vanilla log", err)),
            }
        }
        tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
        Ok(Self {
            child,
            stdin,
            addr,
            log,
            _dir: dir,
        })
    }

    /// Local address of the vanilla listener.
    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Recent stdout lines.
    #[must_use]
    pub fn log_tail(&self) -> String {
        self.log.join("\n")
    }

    /// Stop the server.
    pub async fn shutdown(mut self) -> Result<(), Error> {
        let _ = self.stdin.write_all(b"stop\n").await;
        let _ = self.stdin.flush().await;
        let _ = timeout(Duration::from_secs(30), self.child.wait()).await;
        let _ = self.child.start_kill();
        Ok(())
    }
}

impl Drop for VanillaProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

fn ensure_java25() -> Result<(), Error> {
    let output = java_std()
        .arg("-version")
        .output()
        .map_err(|err| Error::io("run java -version", err))?;
    let text = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{text}{}", String::from_utf8_lossy(&output.stdout));
    let Some(major) = java_major(&combined) else {
        return Err(Error::vanilla(format!(
            "could not parse java -version output: {combined}"
        )));
    };
    if major < 25 {
        return Err(Error::vanilla(format!(
            "Minecraft {CURRENT_MC_VERSION} dedicated server needs Java 25 (found Java {major}). Set JAVA_HOME to a Java 25 install. The jar is not committed.",
        )));
    }
    Ok(())
}

fn java_major(text: &str) -> Option<u32> {
    let marker = text.find("version \"")?;
    let rest = &text[marker + "version \"".len()..];
    let version = rest.split('"').next()?;
    let major = version.split(['.', '_']).next()?;
    major.parse().ok()
}

fn ensure_server_jar() -> Result<PathBuf, Error> {
    let version = CURRENT_MC_VERSION.to_string();
    let cache = cache_dir();
    std::fs::create_dir_all(&cache).map_err(|err| Error::io("create compat cache", err))?;
    let jar_path = cache.join(format!("server-{version}.jar"));
    let manifest = curl_text(MANIFEST_URL)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)
        .map_err(|err| Error::vanilla(format!("version manifest is not json: {err}")))?;
    let versions = manifest
        .get("versions")
        .and_then(|value| value.as_array())
        .ok_or_else(|| Error::vanilla("version manifest has no versions array"))?;
    let entry = versions
        .iter()
        .find(|entry| entry.get("id").and_then(|id| id.as_str()) == Some(version.as_str()))
        .ok_or_else(|| Error::vanilla(format!("version {version} is not in Mojang's manifest")))?;
    let version_url = entry
        .get("url")
        .and_then(|url| url.as_str())
        .ok_or_else(|| Error::vanilla("version entry has no url"))?;
    require_mojang_url(version_url)?;
    let version_json = curl_text(version_url)?;
    let version_json: serde_json::Value = serde_json::from_str(&version_json)
        .map_err(|err| Error::vanilla(format!("version json is not json: {err}")))?;
    let server = version_json
        .get("downloads")
        .and_then(|downloads| downloads.get("server"))
        .ok_or_else(|| Error::vanilla("version json has no downloads.server"))?;
    let jar_url = server
        .get("url")
        .and_then(|url| url.as_str())
        .ok_or_else(|| Error::vanilla("server download has no url"))?;
    let sha1 = server
        .get("sha1")
        .and_then(|sha| sha.as_str())
        .ok_or_else(|| Error::vanilla("server download has no sha1"))?;
    require_mojang_url(jar_url)?;

    if jar_path.is_file() && file_sha1(&jar_path)? == sha1 {
        return Ok(jar_path);
    }
    let bytes = curl_bytes(jar_url)?;
    let got = hex_sha1(&bytes);
    if got != sha1 {
        return Err(Error::vanilla(format!(
            "server jar sha1 was {got}, Mojang published {sha1}"
        )));
    }
    std::fs::write(&jar_path, &bytes).map_err(|err| Error::io("write server jar cache", err))?;
    Ok(jar_path)
}

fn require_mojang_url(url: &str) -> Result<(), Error> {
    let ok = url.starts_with("https://piston-meta.mojang.com/")
        || url.starts_with("https://piston-data.mojang.com/")
        || url.starts_with("https://launchermeta.mojang.com/");
    if ok {
        Ok(())
    } else {
        Err(Error::vanilla(format!(
            "refusing to download from a non-Mojang URL: {url}"
        )))
    }
}

fn cache_dir() -> PathBuf {
    std::env::var_os("PUMPKIN_COMPAT_CACHE").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/compat-cache"),
        PathBuf::from,
    )
}

fn curl_text(url: &str) -> Result<String, Error> {
    let bytes = curl_bytes(url)?;
    String::from_utf8(bytes).map_err(|err| Error::vanilla(format!("download was not utf-8: {err}")))
}

fn curl_bytes(url: &str) -> Result<Vec<u8>, Error> {
    let output = StdCommand::new("curl")
        .args(["-fsSL", "--retry", "3", "--max-time", "180", url])
        .output()
        .map_err(|err| Error::io("run curl", err))?;
    if !output.status.success() {
        return Err(Error::vanilla(format!(
            "curl failed for {url}: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(output.stdout)
}

fn file_sha1(path: &Path) -> Result<String, Error> {
    let bytes = std::fs::read(path).map_err(|err| Error::io("read cached jar", err))?;
    Ok(hex_sha1(&bytes))
}

fn hex_sha1(bytes: &[u8]) -> String {
    let digest = Sha1::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        out.push(char::from(b"0123456789abcdef"[usize::from(byte & 0x0f)]));
    }
    out
}

fn write_world_files(dir: &Path, port: u16) -> Result<(), Error> {
    std::fs::write(dir.join("eula.txt"), "eula=true\n")
        .map_err(|err| Error::io("write eula.txt", err))?;
    let properties = format!(
        "online-mode=false\nwhite-list=false\nenforce-whitelist=false\nserver-port={port}\nmotd=vanilla-compat\nview-distance=2\nsimulation-distance=2\nspawn-protection=0\ngamemode=creative\nlevel-type=minecraft:flat\ngenerate-structures=false\nmax-players=8\nenable-rcon=false\n"
    );
    std::fs::write(dir.join("server.properties"), properties)
        .map_err(|err| Error::io("write server.properties", err))
}

async fn free_port() -> Result<u16, Error> {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .map_err(|err| Error::io("bind ephemeral port", err))?;
    let port = listener
        .local_addr()
        .map_err(|err| Error::io("read ephemeral port", err))?
        .port();
    drop(listener);
    Ok(port)
}

fn java_command() -> Command {
    Command::new(java_bin())
}

fn java_std() -> StdCommand {
    StdCommand::new(java_bin())
}

fn java_bin() -> PathBuf {
    std::env::var_os("JAVA_HOME").map_or_else(
        || PathBuf::from("java"),
        |home| PathBuf::from(home).join("bin").join("java"),
    )
}

fn push_log(log: &mut Vec<String>, line: String) {
    if log.len() == 80 {
        log.remove(0);
    }
    log.push(line);
}

#[cfg(test)]
mod tests {
    use super::java_major;

    #[test]
    fn parses_java_25_and_21_banners() {
        assert_eq!(
            java_major("openjdk version \"25.0.1\" 2025-10-21\n"),
            Some(25)
        );
        assert_eq!(
            java_major("openjdk version \"21.0.10\" 2026-01-20\n"),
            Some(21)
        );
        assert_eq!(java_major("no version here"), None);
    }
}
