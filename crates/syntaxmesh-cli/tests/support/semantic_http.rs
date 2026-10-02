use std::io::Read;
use std::net::TcpStream;
use std::time::Duration;

use serde_json::Value;

pub(super) fn read_request(stream: &mut TcpStream) -> std::io::Result<(String, Value)> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0_u8; 4096];
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Err(std::io::Error::other("incomplete HTTP fixture request"));
        }
        bytes.extend_from_slice(
            buffer
                .get(..count)
                .ok_or_else(|| std::io::Error::other("invalid request length"))?,
        );
        if let Some(boundary) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = std::str::from_utf8(
                bytes
                    .get(..boundary)
                    .ok_or_else(|| std::io::Error::other("invalid HTTP headers"))?,
            )
            .map_err(std::io::Error::other)?;
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>())
                })
                .transpose()
                .map_err(std::io::Error::other)?
                .unwrap_or(0);
            let offset = boundary.saturating_add(4);
            let end = offset.saturating_add(length);
            if bytes.len() >= end {
                let path = headers.lines().next().unwrap_or_default().to_owned();
                let body = if length == 0 {
                    Value::Null
                } else {
                    serde_json::from_slice(
                        bytes
                            .get(offset..end)
                            .ok_or_else(|| std::io::Error::other("invalid HTTP body"))?,
                    )
                    .map_err(std::io::Error::other)?
                };
                return Ok((path, body));
            }
        }
    }
}
