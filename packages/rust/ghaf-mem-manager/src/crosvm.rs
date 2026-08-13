/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */
use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::{path::PathBuf, process::Output, time::Duration};
use tokio::{process::Command, time::timeout};

const TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Deserialize, Debug)]
struct BalloonStatsResponse {
    #[serde(rename = "BalloonStats")]
    balloon_stats: BalloonStats,
}

#[derive(Deserialize, Debug)]
struct BalloonStats {
    stats: GuestMemoryStats,
    balloon_actual: u64,
}

#[derive(Deserialize, Debug)]
struct GuestMemoryStats {
    available_memory: Option<u64>,
    free_memory: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct BalloonInfo {
    pub balloon_actual: usize,
    pub free_memory: usize,
    pub available_memory: usize,
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct CrosvmEndpoint {
    binary: PathBuf,
    socket: PathBuf,
}

impl CrosvmEndpoint {
    pub fn new(binary: impl Into<PathBuf>, socket: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            socket: socket.into(),
        }
    }

    async fn command(&self, arguments: &[&str]) -> Result<Output> {
        let mut command = Command::new(&self.binary);
        command
            .arg("--no-syslog")
            .args(arguments)
            .kill_on_drop(true);
        let output = timeout(TIMEOUT, command.output())
            .await
            .with_context(|| {
                format!(
                    "crosvm `{}` timed out after {TIMEOUT:?}",
                    arguments.join(" ")
                )
            })?
            .with_context(|| {
                format!("failed to execute crosvm binary {}", self.binary.display())
            })?;
        if !output.status.success() {
            // Crosvm reports some control failures on stdout, including
            // unexpected VM responses and failed balloon statistics requests.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let detail = match (stderr.trim(), stdout.trim()) {
                ("", "") => "<no output>".to_owned(),
                ("", out) => out.to_owned(),
                (err, "") => err.to_owned(),
                (err, out) => format!("{err}; stdout: {out}"),
            };
            bail!(
                "crosvm `{}` exited with {}: {detail}",
                arguments.join(" "),
                output.status
            );
        }
        Ok(output)
    }

    pub async fn query_balloon(&self) -> Result<BalloonInfo> {
        let socket = self
            .socket
            .to_str()
            .context("crosvm socket path is not valid UTF-8")?;
        let output = self.command(&["balloon_stats", socket]).await?;
        let response: BalloonStatsResponse = serde_json::from_slice(&output.stdout)
            .context("failed to parse crosvm balloon statistics")?;
        let stats = response.balloon_stats;

        Ok(BalloonInfo {
            balloon_actual: usize::try_from(stats.balloon_actual)
                .context("crosvm balloon size does not fit usize")?,
            free_memory: usize::try_from(stats.stats.free_memory.unwrap_or(0))
                .context("guest free memory does not fit usize")?,
            available_memory: usize::try_from(
                stats
                    .stats
                    .available_memory
                    .ok_or_else(|| anyhow!("guest did not report available memory"))?,
            )
            .context("guest available memory does not fit usize")?,
        })
    }

    pub async fn set_visible_memory(&self, visible: usize, maximum: usize) -> Result<()> {
        if visible > maximum {
            bail!("visible memory target exceeds configured maximum");
        }
        let reclaimed = maximum - visible;
        let reclaimed = reclaimed.to_string();
        let socket = self
            .socket
            .to_str()
            .context("crosvm socket path is not valid UTF-8")?;
        self.command(&["balloon", &reclaimed, socket]).await?;
        Ok(())
    }
}

impl std::fmt::Display for CrosvmEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.socket.display().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    use super::*;

    const MIB: usize = 1024 * 1024;

    /// These tests write an executable and immediately exec it. A concurrent
    /// fork in a sibling test can inherit the still-open write descriptor and
    /// make execve fail with ETXTBSY, so serialise those sections.
    static EXEC_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[tokio::test(flavor = "current_thread")]
    async fn parses_stats_and_translates_visible_memory_target() -> Result<()> {
        let _guard = EXEC_LOCK.lock().await;
        let directory = tempfile::tempdir()?;
        let binary = directory.path().join("crosvm");
        let log = directory.path().join("arguments");
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$@" >> '{}'
if [ "$2" = balloon_stats ]; then
    printf '%s\n' '{{"BalloonStats":{{"stats":{{"available_memory":2147483648,"free_memory":1073741824}},"balloon_actual":4294967296}}}}'
fi
"#,
            log.display()
        );
        fs::write(&binary, script)?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;

        let endpoint = CrosvmEndpoint::new(&binary, Path::new("/run/crosvm.sock"));
        assert_eq!(
            endpoint.query_balloon().await?,
            BalloonInfo {
                balloon_actual: 4096 * MIB,
                free_memory: 1024 * MIB,
                available_memory: 2048 * MIB,
            }
        );
        assert_eq!(
            fs::read_to_string(&log)?,
            "--no-syslog\nballoon_stats\n/run/crosvm.sock\n"
        );
        fs::write(&log, "")?;

        endpoint
            .set_visible_memory(4096 * MIB, 12_288 * MIB)
            .await?;

        assert_eq!(
            fs::read_to_string(log)?,
            "--no-syslog\nballoon\n8589934592\n/run/crosvm.sock\n"
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_missing_available_memory() -> Result<()> {
        let _guard = EXEC_LOCK.lock().await;
        let directory = tempfile::tempdir()?;
        let binary = directory.path().join("crosvm");
        fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s\\n' '{\"BalloonStats\":{\"stats\":{},\"balloon_actual\":0}}'\n",
        )?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;

        let endpoint = CrosvmEndpoint::new(&binary, Path::new("/run/crosvm.sock"));
        let message = endpoint.query_balloon().await.unwrap_err().to_string();
        assert!(
            message.contains("available memory"),
            "unexpected error: {message}"
        );
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reports_crosvm_stdout_on_command_failure() -> Result<()> {
        let _guard = EXEC_LOCK.lock().await;
        let directory = tempfile::tempdir()?;
        let binary = directory.path().join("crosvm");
        fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s\\n' 'unexpected response: connection refused'\nexit 1\n",
        )?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))?;

        let endpoint = CrosvmEndpoint::new(&binary, Path::new("/run/crosvm.sock"));
        let message = endpoint.query_balloon().await.unwrap_err().to_string();
        assert!(
            message.contains("unexpected response: connection refused"),
            "unexpected error: {message}"
        );
        Ok(())
    }
}
