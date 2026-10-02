use std::time::{Duration, SystemTime};

use reqwest::header::{HeaderMap, RETRY_AFTER};

#[cfg(test)]
mod tests;

pub(super) fn delay(headers: &HeaderMap, attempt: u32) -> Result<Duration, String> {
    delay_at(headers, attempt, SystemTime::now())
}

fn delay_at(headers: &HeaderMap, attempt: u32, now: SystemTime) -> Result<Duration, String> {
    if let Some(value) = headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
    {
        let duration = if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
            let seconds = value.parse::<u64>().map_err(|_error| {
                "semantic provider Retry-After exceeds the synchronous retry limit".to_owned()
            })?;
            Some(Duration::from_secs(seconds))
        } else {
            httpdate::parse_http_date(value)
                .ok()
                .map(|date| date.duration_since(now).unwrap_or_default())
        };
        if let Some(duration) = duration {
            if duration > Duration::from_secs(5) {
                return Err("semantic provider requested Retry-After beyond the 5 second synchronous retry limit; retry the durable job later".to_owned());
            }
            return Ok(duration);
        }
    }
    Ok(Duration::from_millis(250_u64 << attempt.min(1)))
}
