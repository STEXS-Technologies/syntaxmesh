//! Bounded stdin/stdout transport for caller-configured local AI harnesses.

use std::io::{Read, Seek, SeekFrom, Write};
use std::process::{Child, Command, Stdio};
use std::time::Instant;

use super::{MAX_RESPONSE_BYTES, REQUEST_TIMEOUT};

#[cfg(test)]
mod tests;

pub(super) fn validate(argv: &[String]) -> Result<(), String> {
    if argv.is_empty()
        || argv.len() > 128
        || argv
            .first()
            .is_none_or(|executable| executable.trim().is_empty())
        || argv
            .iter()
            .any(|argument| argument.contains('\0') || argument.len() > 64 * 1024)
    {
        return Err("semantic command requires 1–128 bounded, NUL-free argv strings and a non-empty executable".to_owned());
    }
    Ok(())
}

struct RunningChild(Child);

impl Drop for RunningChild {
    fn drop(&mut self) {
        if self.0.try_wait().is_ok_and(|status| status.is_some()) {
            return;
        }
        drop(self.0.kill());
        drop(self.0.wait());
    }
}

pub(super) fn run(argv: &[String], prompt: &str) -> Result<String, String> {
    validate(argv)?;
    let executable = argv
        .first()
        .ok_or_else(|| "missing semantic command".to_owned())?;
    let directory =
        tempfile::tempdir().map_err(|_error| "could not isolate semantic command".to_owned())?;
    let mut input = tempfile::tempfile()
        .map_err(|_error| "could not stage semantic command input".to_owned())?;
    input
        .write_all(prompt.as_bytes())
        .map_err(|_error| "could not write semantic command input".to_owned())?;
    input
        .seek(SeekFrom::Start(0))
        .map_err(|_error| "could not rewind semantic command input".to_owned())?;
    let mut output = tempfile::tempfile()
        .map_err(|_error| "could not capture semantic command output".to_owned())?;
    let diagnostics = tempfile::tempfile()
        .map_err(|_error| "could not capture semantic command diagnostics".to_owned())?;
    let output_handle = output
        .try_clone()
        .map_err(|_error| "could not capture semantic command output".to_owned())?;
    let diagnostic_handle = diagnostics
        .try_clone()
        .map_err(|_error| "could not capture semantic command diagnostics".to_owned())?;
    let mut child = RunningChild(
        Command::new(executable)
            .args(argv.iter().skip(1))
            .current_dir(directory.path())
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output_handle))
            .stderr(Stdio::from(diagnostic_handle))
            .spawn()
            .map_err(|_error| {
                "could not launch semantic command; check the executable and harness login"
                    .to_owned()
            })?,
    );
    let started = Instant::now();
    loop {
        if output
            .metadata()
            .map_err(|_error| "could not inspect command output".to_owned())?
            .len()
            > MAX_RESPONSE_BYTES as u64
            || diagnostics
                .metadata()
                .map_err(|_error| "could not inspect command diagnostics".to_owned())?
                .len()
                > MAX_RESPONSE_BYTES as u64
        {
            return Err("semantic command exceeded the 4 MiB output/diagnostic limit".to_owned());
        }
        if let Some(status) = child
            .0
            .try_wait()
            .map_err(|_error| "could not wait for semantic command".to_owned())?
        {
            if !status.success() {
                return Err(format!(
                    "semantic command failed with {status}; accepted semantic history was not replaced"
                ));
            }
            break;
        }
        if started.elapsed() >= REQUEST_TIMEOUT {
            return Err("semantic command timed out after 180 seconds".to_owned());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    output
        .seek(SeekFrom::Start(0))
        .map_err(|_error| "could not rewind command output".to_owned())?;
    if diagnostics
        .metadata()
        .map_err(|_error| "could not inspect command diagnostics".to_owned())?
        .len()
        > MAX_RESPONSE_BYTES as u64
    {
        return Err("semantic command diagnostics exceeded the 4 MiB limit".to_owned());
    }
    let mut bytes = Vec::new();
    output
        .take(MAX_RESPONSE_BYTES.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_error| "could not read semantic command output".to_owned())?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("semantic command output exceeded the 4 MiB limit".to_owned());
    }
    String::from_utf8(bytes).map_err(|_error| "semantic command output was not UTF-8".to_owned())
}
