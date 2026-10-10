//! Spawn a scratch Pumpkin server for one scenario run.
//!
//! The process writes its world and config into a temporary directory. Nothing
//! from Mojang is stored there.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::{Child, ChildStdin, Command};
use tokio::time::timeout;

use crate::error::Error;

const READY_LINE: &str = "Server is now running";
const READY_TIMEOUT: Duration = Duration::from_secs(300);

/// A Pumpkin process bound to localhost with offline mode and creative mode.
pub struct PumpkinProcess {
    child: Child,
    stdin: ChildStdin,
    addr: SocketAddr,
    log: Arc<Mutex<Vec<String>>>,
    _dir: TempDir,
}

impl PumpkinProcess {
    /// Build `pumpkin.toml`, start `binary`, and wait until it accepts joins.
    pub async fn start(binary: &Path) -> Result<Self, Error> {
        if !binary.is_file() {
            return Err(Error::missing_binary(binary));
        }
        let dir = TempDir::new().map_err(|err| Error::io("create scratch dir", err))?;
        let port = free_port().await?;
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        write_config(dir.path(), port)?;

        let mut command = Command::new(binary);
        command
            .current_dir(dir.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .map_err(|err| Error::io("spawn pumpkin", err))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::scenario("pumpkin was spawned without stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::scenario("pumpkin was spawned without stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::scenario("pumpkin was spawned without stderr"))?;

        let log = Arc::new(Mutex::new(Vec::new()));
        let stderr_log = Arc::clone(&log);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                push_log(&stderr_log, line);
            }
        });

        let mut lines = BufReader::new(stdout).lines();
        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::scenario(format!(
                    "pumpkin did not report ready within {}s\n{}",
                    READY_TIMEOUT.as_secs(),
                    tail(&log)
                )));
            }
            match timeout(remaining, lines.next_line()).await {
                Ok(Ok(Some(line))) => {
                    let ready = line.contains(READY_LINE);
                    let failed = line.contains("already in use");
                    push_log(&log, line);
                    if failed {
                        return Err(Error::scenario(format!(
                            "pumpkin failed to bind\n{}",
                            tail(&log)
                        )));
                    }
                    if ready {
                        break;
                    }
                }
                Ok(Ok(None)) => {
                    return Err(Error::scenario(format!(
                        "pumpkin exited before it was ready\n{}",
                        tail(&log)
                    )));
                }
                Ok(Err(err)) => return Err(Error::io("read pumpkin log", err)),
                Err(_) => {
                    return Err(Error::scenario(format!(
                        "pumpkin did not report ready within {}s\n{}",
                        READY_TIMEOUT.as_secs(),
                        tail(&log)
                    )));
                }
            }
        }

        // Keep draining stdout so a full pipe cannot stall the server.
        let rest_log = Arc::clone(&log);
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                push_log(&rest_log, line);
            }
        });

        Ok(Self {
            child,
            stdin,
            addr,
            log,
            _dir: dir,
        })
    }

    /// Address the Java listener was bound to.
    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Recent log lines, for scenario failures.
    #[must_use]
    pub fn log_tail(&self) -> String {
        tail(&self.log)
    }

    /// Send one console command. The line must not contain a newline.
    pub async fn command(&mut self, line: &str) -> Result<(), Error> {
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|err| Error::io("write console command", err))?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|err| Error::io("write console newline", err))?;
        self.stdin
            .flush()
            .await
            .map_err(|err| Error::io("flush console", err))
    }

    /// Ask the server to stop, then kill it if it is still running.
    pub async fn shutdown(mut self) -> Result<(), Error> {
        let _ = self.stdin.write_all(b"stop\n").await;
        let _ = self.stdin.flush().await;
        match timeout(Duration::from_secs(20), self.child.wait()).await {
            Ok(Ok(_)) | Err(_) => {
                let _ = self.child.start_kill();
                Ok(())
            }
            Ok(Err(err)) => Err(Error::io("wait for pumpkin", err)),
        }
    }
}

impl Drop for PumpkinProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// `PUMPKIN_BIN`, or `target/debug/pumpkin` next to this crate.
pub fn pumpkin_binary() -> Result<PathBuf, Error> {
    if let Some(raw) = std::env::var_os("PUMPKIN_BIN") {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Ok(path);
        }
        return Err(Error::missing_binary(&path));
    }
    let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/pumpkin");
    if candidate.is_file() {
        return Ok(candidate);
    }
    Err(Error::missing_binary(&candidate))
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

fn write_config(dir: &Path, port: u16) -> Result<(), Error> {
    let toml = format!(
        r#"
seed = "1"
default_gamemode = "Creative"
force_gamemode = true
spawn_protection = 0
allow_nether = false
allow_end = false

[logging]
level = "info"
color = false

[telemetry]
enabled = false

[commands]
use_console = true
use_tty = false

[networking.java]
enabled = true
address = "127.0.0.1:{port}"
encryption = false
online_mode = false
max_players = 8
view_distance = 2
simulation_distance = 2
keep_alive_time = 120
motd = "pumpkin-compat"

[networking.java.compression]
enabled = true
threshold = 256
level = 1

[networking.bedrock]
enabled = false

[networking.lan_broadcast]
enabled = false

[networking.query]
enabled = false

[networking.management]
enabled = false
"#
    );
    std::fs::write(dir.join("pumpkin.toml"), toml)
        .map_err(|err| Error::io("write pumpkin.toml", err))
}

fn push_log(log: &Mutex<Vec<String>>, line: String) {
    let mut guard = log
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if guard.len() == 80 {
        guard.remove(0);
    }
    guard.push(line);
}

fn tail(log: &Mutex<Vec<String>>) -> String {
    let guard = log
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.join("\n")
}
