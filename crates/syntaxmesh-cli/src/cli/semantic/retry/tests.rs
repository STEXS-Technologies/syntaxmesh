use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};

use super::delay;

#[test]
fn http_date_delays_use_exact_bounded_clock_difference() -> Result<(), Box<dyn std::error::Error>> {
    let now = httpdate::parse_http_date("Wed, 30 Sep 2026 12:00:00 GMT")?;
    let mut headers = HeaderMap::new();
    for (value, seconds) in [
        ("Wed, 30 Sep 2026 11:59:59 GMT", 0),
        ("Wed, 30 Sep 2026 12:00:00 GMT", 0),
        ("Wed, 30 Sep 2026 12:00:03 GMT", 3),
        ("Wed, 30 Sep 2026 12:00:05 GMT", 5),
    ] {
        headers.insert(RETRY_AFTER, HeaderValue::from_static(value));
        if super::delay_at(&headers, 0, now)? != Duration::from_secs(seconds) {
            return Err(std::io::Error::other("HTTP date delay differs from fixed clock").into());
        }
    }
    headers.insert(
        RETRY_AFTER,
        HeaderValue::from_static("Wed, 30 Sep 2026 12:00:06 GMT"),
    );
    if super::delay_at(&headers, 0, now).is_ok() {
        return Err(std::io::Error::other("long HTTP date bypassed retry limit").into());
    }
    Ok(())
}

#[test]
fn provider_delay_precedes_bounded_fallback() {
    let mut headers = HeaderMap::new();
    assert_eq!(delay(&headers, 0), Ok(Duration::from_millis(250)));
    assert_eq!(delay(&headers, u32::MAX), Ok(Duration::from_millis(500)));
    for (value, seconds) in [("0", 0), ("2", 2), ("5", 5)] {
        headers.insert(RETRY_AFTER, HeaderValue::from_static(value));
        assert_eq!(delay(&headers, 0), Ok(Duration::from_secs(seconds)));
    }
    for value in ["", "invalid", "-1", "+1", "1.5"] {
        headers.insert(RETRY_AFTER, HeaderValue::from_static(value));
        assert_eq!(delay(&headers, 0), Ok(Duration::from_millis(250)));
    }
    headers.insert(RETRY_AFTER, HeaderValue::from_static("6"));
    assert!(delay(&headers, 0).is_err());
    headers.insert(
        RETRY_AFTER,
        HeaderValue::from_static("999999999999999999999999999999999999"),
    );
    assert!(delay(&headers, 0).is_err());
}

#[test]
fn http_retry_honors_zero_delay_and_defers_excessive_delay()
-> Result<(), Box<dyn std::error::Error>> {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Instant;

    use super::super::OpenAiCompatibleProvider;

    for (responses, expected_requests, succeeds) in [
        (vec![(429, "0"), (200, "")], 2, true),
        (vec![(503, "6")], 1, false),
        (vec![(401, "0")], 1, false),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let endpoint = format!("http://{}/v1", listener.local_addr()?);
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            for (status, delay) in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(error) => return Err(error),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request)?;
                let body = "{\"choices\":[]}";
                write!(
                    stream,
                    "HTTP/1.1 {status} Fixture\r\nRetry-After: {delay}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )?;
            }
            Ok(())
        });
        let provider = OpenAiCompatibleProvider::new(Some(&endpoint), "fixture", None, false)?;
        let result = provider.send_request(&serde_json::json!({}));
        server
            .join()
            .map_err(|_panic| std::io::Error::other("retry fixture panicked"))??;
        if result.is_ok() != succeeds
            || provider.requests_made() != expected_requests
            || !provider
                .usage_summary()
                .contains(&format!("missing={expected_requests}"))
            || (!succeeds
                && expected_requests == 1
                && !result.is_err_and(|error| {
                    error.contains("retry limit") || error.contains("HTTP 401")
                }))
        {
            return Err(std::io::Error::other(
                "provider-directed retry behavior or counters differ",
            )
            .into());
        }
    }
    Ok(())
}
