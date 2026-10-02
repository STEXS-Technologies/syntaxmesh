use std::io::{Read, Write};

use syntaxmesh_extension_sdk::{ExtensionError, ExtensionManifest, FactBatch};

pub const WIRE_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAGIC: &[u8; 4] = b"SMEX";

/// Bounded framing for an externally owned stream.
/// Discard the stream after any error; this codec never resynchronizes it.
#[derive(Debug, Clone, Copy)]
pub struct FrameCodec {
    max_bytes: usize,
}

impl FrameCodec {
    /// Select a payload limit in 1..=MAX_FRAME_BYTES.
    ///
    /// # Errors
    /// Rejects zero or a limit exceeding the hard protocol cap.
    pub const fn new(max_bytes: usize) -> Result<Self, FrameError> {
        if max_bytes == 0 || max_bytes > MAX_FRAME_BYTES {
            return Err(FrameError::InvalidLimit);
        }
        Ok(Self { max_bytes })
    }

    /// Read one validated batch; None means clean EOF before the next header.
    ///
    /// # Errors
    /// Rejects incomplete, unsupported, oversized, malformed, or invalid frames.
    /// Timeouts and peer authorization belong to the stream owner.
    pub fn read_batch<R: Read>(&self, reader: &mut R) -> Result<Option<FactBatch>, FrameError> {
        let mut header = [0_u8; 12];
        loop {
            match reader.read(&mut header[..1]) {
                Ok(0) => return Ok(None),
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(FrameError::Io(error)),
            }
        }
        reader
            .read_exact(&mut header[1..])
            .map_err(FrameError::Io)?;
        if &header[..4] != MAGIC {
            return Err(FrameError::InvalidMagic);
        }
        let version = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
        if version != WIRE_VERSION {
            return Err(FrameError::UnsupportedVersion(version));
        }
        let length = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
        let length = usize::try_from(length).map_err(|_error| FrameError::PayloadLimit)?;
        if length == 0 || length > self.max_bytes {
            return Err(FrameError::PayloadLimit);
        }
        let mut payload = vec![0; length];
        reader.read_exact(&mut payload).map_err(FrameError::Io)?;
        let batch: FactBatch = serde_json::from_slice(&payload).map_err(FrameError::Json)?;
        batch.validate().map_err(FrameError::InvalidBatch)?;
        Ok(Some(batch))
    }

    /// Decode a batch restricted to an independently authorized producer grant.
    /// The caller authenticates the peer and supplies the grant; never copy it
    /// from incoming data. This check is not peer authentication or attestation.
    ///
    /// # Errors
    /// Returns framing/validation errors or rejects a namespace, producer version,
    /// or declared capability not authorized by the grant. Discard failed streams.
    pub fn read_batch_for<R: Read>(
        &self,
        reader: &mut R,
        grant: &ExtensionManifest,
    ) -> Result<Option<FactBatch>, FrameError> {
        grant.validate().map_err(FrameError::InvalidBatch)?;
        let Some(batch) = self.read_batch(reader)? else {
            return Ok(None);
        };
        if batch.manifest.namespace != grant.namespace
            || batch.manifest.producer_version != grant.producer_version
            || batch
                .manifest
                .capabilities
                .iter()
                .any(|capability| !grant.capabilities.contains(capability))
        {
            return Err(FrameError::UnauthorizedManifest);
        }
        Ok(Some(batch))
    }

    /// Validate and write one frame. Does not flush an externally owned stream.
    ///
    /// # Errors
    /// Invalid or oversized batches fail before any frame bytes are written.
    /// A failed underlying write may leave a partial frame; discard the stream.
    pub fn write_batch<W: Write>(
        &self,
        writer: &mut W,
        batch: &FactBatch,
    ) -> Result<(), FrameError> {
        batch.validate().map_err(FrameError::InvalidBatch)?;
        let mut payload = LimitedPayload {
            bytes: Vec::new(),
            limit: self.max_bytes,
            exceeded: false,
        };
        if let Err(error) = serde_json::to_writer(&mut payload, batch) {
            if payload.exceeded {
                return Err(FrameError::PayloadLimit);
            }
            return Err(FrameError::Json(error));
        }
        let length =
            u32::try_from(payload.bytes.len()).map_err(|_error| FrameError::PayloadLimit)?;
        writer.write_all(MAGIC).map_err(FrameError::Io)?;
        writer
            .write_all(&WIRE_VERSION.to_be_bytes())
            .map_err(FrameError::Io)?;
        writer
            .write_all(&length.to_be_bytes())
            .map_err(FrameError::Io)?;
        writer.write_all(&payload.bytes).map_err(FrameError::Io)
    }
}

struct LimitedPayload {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}

impl Write for LimitedPayload {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "extension frame payload limit exceeded",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum FrameError {
    UnauthorizedManifest,
    InvalidLimit,
    InvalidMagic,
    UnsupportedVersion(u32),
    PayloadLimit,
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidBatch(ExtensionError),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnauthorizedManifest => {
                formatter.write_str("extension manifest exceeds its host-authorized producer grant")
            }
            Self::InvalidLimit => formatter.write_str("extension frame limit must be 1..=4 MiB"),
            Self::InvalidMagic => formatter.write_str("invalid extension frame magic"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported extension wire version {version}")
            }
            Self::PayloadLimit => {
                formatter.write_str("extension frame payload is empty or exceeds its limit")
            }
            Self::Io(error) => write!(formatter, "extension frame I/O: {error}"),
            Self::Json(error) => write!(formatter, "extension frame JSON: {error}"),
            Self::InvalidBatch(error) => write!(formatter, "invalid extension batch: {error}"),
        }
    }
}

impl std::error::Error for FrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::InvalidBatch(error) => Some(error),
            Self::InvalidLimit
            | Self::UnauthorizedManifest
            | Self::InvalidMagic
            | Self::UnsupportedVersion(_)
            | Self::PayloadLimit => None,
        }
    }
}

#[cfg(test)]
mod tests;
