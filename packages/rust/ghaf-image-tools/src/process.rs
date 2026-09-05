// SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};
use std::{
    io::Read,
    process::{Child, Command, Stdio},
};

pub fn run(command: &mut Command) -> Result<()> {
    let status = command
        .status()
        .with_context(|| format!("start {}", command.get_program().display()))?;
    ensure!(
        status.success(),
        "{} failed: {status}",
        command.get_program().display()
    );
    Ok(())
}

pub fn output(command: &mut Command) -> Result<String> {
    consume(command, |stdout| {
        let mut text = String::new();
        stdout.read_to_string(&mut text)?;
        Ok(text)
    })
}

/// A failed consumer must terminate and reap its producer, including when the
/// producer fills its stdout pipe. Successful consumption also checks its exit.
pub fn consume<T>(
    command: &mut Command,
    consumer: impl FnOnce(&mut dyn Read) -> Result<T>,
) -> Result<T> {
    let mut child = ReapOnDrop(
        command
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| format!("start {}", command.get_program().display()))?,
    );
    let mut stdout = child.0.stdout.take().context("missing producer stdout")?;
    let result = consumer(&mut stdout)?;
    drop(stdout);
    let status = child.0.wait().context("wait for producer")?;
    ensure!(
        status.success(),
        "{} failed: {status}",
        command.get_program().display()
    );
    Ok(result)
}

struct ReapOnDrop(Child);

impl Drop for ReapOnDrop {
    fn drop(&mut self) {
        // Child::drop does not reap. kill is harmless when wait already finished.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;

    #[test]
    fn rejects_producer_failure_even_after_valid_stdout() {
        let result = consume(
            Command::new("sh").args(["-c", "printf data; exit 7"]),
            |stream| {
                let mut data = String::new();
                stream.read_to_string(&mut data)?;
                assert_eq!(data, "data");
                Ok(())
            },
        );
        assert!(result.unwrap_err().to_string().contains("exit status: 7"));
    }

    #[test]
    fn consumer_failure_reaps_a_blocked_producer() {
        let result: Result<()> = consume(
            Command::new("sh").args(["-c", "while :; do printf data; done"]),
            |_| bail!("consumer failed"),
        );
        assert_eq!(result.unwrap_err().to_string(), "consumer failed");
    }
}
