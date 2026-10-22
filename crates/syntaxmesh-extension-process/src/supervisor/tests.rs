use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use syntaxmesh_extension_ipc::MAX_FRAME_BYTES;

use super::{CaptureError, ProcessLimits, ProcessSupervisor, capture_stderr, capture_stdout};

#[test]
fn validates_supervisor_limits() -> Result<(), Box<dyn std::error::Error>> {
    if ProcessSupervisor::new(ProcessLimits {
        timeout: Duration::ZERO,
        stdout_bytes: 1,
        stderr_bytes: 1,
    })
    .is_ok()
        || ProcessSupervisor::new(ProcessLimits {
            timeout: Duration::from_secs(1),
            stdout_bytes: MAX_FRAME_BYTES + 13,
            stderr_bytes: 1,
        })
        .is_ok()
    {
        return Err("process supervisor accepted invalid limits".into());
    }
    Ok(())
}

#[test]
fn stdout_capture_fails_closed_at_the_selected_byte_limit() -> Result<(), Box<dyn std::error::Error>>
{
    let overflow = Arc::new(AtomicBool::new(false));
    let result = capture_stdout(Cursor::new([1_u8, 2, 3, 4]), 3, &overflow);
    if !matches!(result, Err(CaptureError::LimitExceeded)) || !overflow.load(Ordering::Acquire) {
        return Err("stdout capture did not report its byte limit".into());
    }
    let accepted = capture_stdout(Cursor::new([1_u8, 2, 3]), 3, &overflow)
        .map_err(|_error| "stdout capture rejected data at the exact limit")?;
    if accepted != [1, 2, 3] {
        return Err("stdout capture changed data at its exact limit".into());
    }
    Ok(())
}

#[test]
fn stderr_capture_truncates_retention_but_drains_the_stream()
-> Result<(), Box<dyn std::error::Error>> {
    let captured = capture_stderr(Cursor::new([1_u8, 2, 3, 4]), 2)?;
    if captured.bytes != [1, 2] || !captured.truncated {
        return Err("stderr capture did not enforce its retained-byte cap".into());
    }
    Ok(())
}
