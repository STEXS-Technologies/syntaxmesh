use std::ffi::OsStr;
use std::io::{self, Cursor, Read};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use command_group::CommandGroup;
use syntaxmesh_extension_ipc::{FrameCodec, FrameError, MAX_FRAME_BYTES};
use syntaxmesh_extension_sdk::{ExtensionManifest, FactBatch};

#[cfg(test)]
mod tests;

const FRAME_HEADER_BYTES: usize = 12;
const HARD_STDOUT_BYTES: usize = MAX_FRAME_BYTES + FRAME_HEADER_BYTES;
const HARD_STDERR_BYTES: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Caller-selected process and capture budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
}

impl ProcessLimits {
    /// Construct conservative defaults for a one-frame producer invocation.
    #[must_use]
    pub const fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            stdout_bytes: HARD_STDOUT_BYTES,
            stderr_bytes: 16 * 1024,
        }
    }
}

/// A validated producer batch and bounded process diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessReceipt {
    pub batch: FactBatch,
    pub elapsed: Duration,
    pub stdout_bytes: usize,
    pub stderr: Vec<u8>,
    pub stderr_truncated: bool,
}

/// Runs producer commands in an isolated process group under explicit budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessSupervisor {
    limits: ProcessLimits,
}

/// Explicit operator-selected producer command with ambient environment
/// inheritance disabled by default.
pub struct ProducerCommand {
    command: Command,
}

impl ProducerCommand {
    /// Select one executable and start with an empty child environment.
    #[must_use]
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        let mut command = Command::new(program);
        command.env_clear();
        Self { command }
    }

    /// Add one producer argument without invoking a shell.
    pub fn arg(&mut self, argument: impl AsRef<OsStr>) -> &mut Self {
        self.command.arg(argument);
        self
    }

    /// Add producer arguments without invoking a shell.
    pub fn args<I, S>(&mut self, arguments: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command.args(arguments);
        self
    }

    /// Allow one explicitly selected environment value into the child.
    pub fn env(&mut self, name: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> &mut Self {
        self.command.env(name, value);
        self
    }
}

impl ProcessSupervisor {
    /// Create a supervisor with bounded stdout/stderr and a positive deadline.
    ///
    /// # Errors
    /// Rejects zero deadlines, zero limits, and limits above protocol hard caps.
    pub const fn new(limits: ProcessLimits) -> Result<Self, SupervisionError> {
        if limits.timeout.is_zero()
            || limits.stdout_bytes == 0
            || limits.stdout_bytes > HARD_STDOUT_BYTES
            || limits.stderr_bytes == 0
            || limits.stderr_bytes > HARD_STDERR_BYTES
        {
            return Err(SupervisionError::InvalidLimits);
        }
        Ok(Self { limits })
    }

    /// Launch one externally configured producer, enforce its deadline and
    /// process-group cleanup, then validate exactly one authorized frame.
    ///
    /// Executable selection, arguments, working directory, and environment are
    /// supplied by the host and must come from operator policy. The process
    /// adapter does not infer trust from the producer's manifest.
    ///
    /// # Errors
    /// Returns spawn/read/wait failures, timeout, output-limit or exit errors,
    /// missing/trailing frame bytes, or authorization/framing validation errors.
    pub fn run_batch(
        &self,
        command: &mut ProducerCommand,
        grant: &ExtensionManifest,
    ) -> Result<ProcessReceipt, SupervisionError> {
        let started = Instant::now();
        let deadline = started
            .checked_add(self.limits.timeout)
            .ok_or(SupervisionError::InvalidLimits)?;
        command
            .command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .command
            .group_spawn()
            .map_err(SupervisionError::Spawn)?;
        let stdout = child
            .inner()
            .stdout
            .take()
            .ok_or(SupervisionError::MissingPipe)?;
        let stderr = child
            .inner()
            .stderr
            .take()
            .ok_or(SupervisionError::MissingPipe)?;

        let stdout_overflow = Arc::new(AtomicBool::new(false));
        let reader_overflow = Arc::clone(&stdout_overflow);
        let stdout_limit = self.limits.stdout_bytes;
        let stdout_reader =
            thread::spawn(move || capture_stdout(stdout, stdout_limit, reader_overflow.as_ref()));
        let stderr_limit = self.limits.stderr_bytes;
        let stderr_reader = thread::spawn(move || capture_stderr(stderr, stderr_limit));

        let (status, timed_out, output_limited) = loop {
            if stdout_overflow.load(Ordering::Acquire) {
                let _kill_result = child.kill();
                let status = child.wait().map_err(SupervisionError::Wait)?;
                break (status, false, true);
            }
            if Instant::now() >= deadline {
                let _kill_result = child.kill();
                let status = child.wait().map_err(SupervisionError::Wait)?;
                break (status, true, false);
            }
            if let Some(status) = child.try_wait().map_err(SupervisionError::Wait)? {
                // End any descendants that inherited the protocol pipes before
                // joining bounded readers. A producer invocation is one-shot.
                let _kill_result = child.kill();
                let group_status = child.wait().map_err(SupervisionError::Wait)?;
                let _leader_status = status;
                break (group_status, false, false);
            }
            thread::sleep(POLL_INTERVAL);
        };

        let stdout_result = stdout_reader
            .join()
            .map_err(|_panic| SupervisionError::ReaderPanicked)?;
        let stderr_capture = stderr_reader
            .join()
            .map_err(|_panic| SupervisionError::ReaderPanicked)?
            .map_err(SupervisionError::Read)?;
        if timed_out {
            return Err(SupervisionError::TimedOut {
                timeout: self.limits.timeout,
                stderr: stderr_capture.bytes,
                stderr_truncated: stderr_capture.truncated,
            });
        }
        if output_limited || matches!(stdout_result, Err(CaptureError::LimitExceeded)) {
            return Err(SupervisionError::StdoutLimit);
        }
        let captured_stdout = stdout_result.map_err(SupervisionError::from)?;
        if !status.success() {
            return Err(SupervisionError::ExitFailure {
                code: status.code(),
                stderr: stderr_capture.bytes,
                stderr_truncated: stderr_capture.truncated,
            });
        }

        let stdout_bytes = captured_stdout.len();
        let mut framed = Cursor::new(captured_stdout);
        let batch = FrameCodec::new(MAX_FRAME_BYTES)
            .map_err(SupervisionError::Frame)?
            .read_batch_for(&mut framed, grant)
            .map_err(SupervisionError::Frame)?
            .ok_or(SupervisionError::MissingFrame)?;
        if usize::try_from(framed.position()).map_err(|_error| SupervisionError::StdoutLimit)?
            != stdout_bytes
        {
            return Err(SupervisionError::TrailingFrameBytes);
        }
        Ok(ProcessReceipt {
            batch,
            elapsed: started.elapsed(),
            stdout_bytes,
            stderr: stderr_capture.bytes,
            stderr_truncated: stderr_capture.truncated,
        })
    }
}

struct CapturedStderr {
    bytes: Vec<u8>,
    truncated: bool,
}

fn capture_stdout<R: Read>(
    mut reader: R,
    limit: usize,
    overflow: &AtomicBool,
) -> Result<Vec<u8>, CaptureError> {
    let mut bytes = Vec::with_capacity(limit.min(16 * 1024));
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = reader.read(&mut buffer).map_err(CaptureError::Read)?;
        if count == 0 {
            return Ok(bytes);
        }
        let Some(next_len) = bytes.len().checked_add(count) else {
            overflow.store(true, Ordering::Release);
            return Err(CaptureError::LimitExceeded);
        };
        if next_len > limit {
            overflow.store(true, Ordering::Release);
            return Err(CaptureError::LimitExceeded);
        }
        let chunk = buffer
            .get(..count)
            .ok_or_else(|| CaptureError::Read(io::Error::other("invalid stdout read length")))?;
        bytes.extend_from_slice(chunk);
    }
}

fn capture_stderr<R: Read>(mut reader: R, limit: usize) -> io::Result<CapturedStderr> {
    let mut bytes = Vec::with_capacity(limit.min(8 * 1024));
    let mut truncated = false;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok(CapturedStderr { bytes, truncated });
        }
        let remaining = limit.saturating_sub(bytes.len());
        let retained = count.min(remaining);
        let chunk = buffer
            .get(..retained)
            .ok_or_else(|| io::Error::other("invalid stderr read length"))?;
        bytes.extend_from_slice(chunk);
        truncated |= retained < count;
    }
}

enum CaptureError {
    LimitExceeded,
    Read(io::Error),
}

impl From<CaptureError> for SupervisionError {
    fn from(error: CaptureError) -> Self {
        match error {
            CaptureError::LimitExceeded => Self::StdoutLimit,
            CaptureError::Read(error) => Self::Read(error),
        }
    }
}

/// Failures while launching, supervising, and validating one producer process.
#[derive(Debug)]
pub enum SupervisionError {
    InvalidLimits,
    MissingPipe,
    Spawn(io::Error),
    Wait(io::Error),
    Read(io::Error),
    ReaderPanicked,
    TimedOut {
        timeout: Duration,
        stderr: Vec<u8>,
        stderr_truncated: bool,
    },
    StdoutLimit,
    ExitFailure {
        code: Option<i32>,
        stderr: Vec<u8>,
        stderr_truncated: bool,
    },
    MissingFrame,
    TrailingFrameBytes,
    Frame(FrameError),
}

impl std::fmt::Display for SupervisionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLimits => formatter.write_str("invalid extension process limits"),
            Self::MissingPipe => formatter.write_str("extension process pipe was not captured"),
            Self::Spawn(error) => write!(formatter, "extension process spawn failed: {error}"),
            Self::Wait(error) => write!(formatter, "extension process wait failed: {error}"),
            Self::Read(error) => write!(formatter, "extension process pipe read failed: {error}"),
            Self::ReaderPanicked => formatter.write_str("extension process pipe reader panicked"),
            Self::TimedOut {
                timeout,
                stderr,
                stderr_truncated,
            } => write!(
                formatter,
                "extension process exceeded {:?}; stderr={}{}",
                timeout,
                String::from_utf8_lossy(stderr),
                if *stderr_truncated {
                    " [truncated]"
                } else {
                    ""
                },
            ),
            Self::StdoutLimit => formatter.write_str("extension process stdout exceeded its limit"),
            Self::ExitFailure {
                code,
                stderr,
                stderr_truncated,
            } => write!(
                formatter,
                "extension process exited unsuccessfully ({code:?}); stderr={}{}",
                String::from_utf8_lossy(stderr),
                if *stderr_truncated {
                    " [truncated]"
                } else {
                    ""
                },
            ),
            Self::MissingFrame => formatter.write_str("extension process emitted no frame"),
            Self::TrailingFrameBytes => {
                formatter.write_str("extension process emitted bytes after its one permitted frame")
            }
            Self::Frame(error) => write!(formatter, "extension process frame rejected: {error}"),
        }
    }
}

impl std::error::Error for SupervisionError {}
