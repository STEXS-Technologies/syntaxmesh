#![allow(clippy::indexing_slicing, clippy::panic_in_result_fn)]

use std::io::{Cursor, Read};

use syntaxmesh_extension_sdk::{
    Capability, EXTENSION_MANIFEST_SCHEMA_VERSION, ExtensionManifest, FactBatch,
};

use super::{FrameCodec, FrameError, MAX_FRAME_BYTES, WIRE_VERSION};

fn batch() -> FactBatch {
    FactBatch {
        manifest: ExtensionManifest {
            schema_version: EXTENSION_MANIFEST_SCHEMA_VERSION,
            namespace: "example.external".to_owned(),
            producer_version: "1".to_owned(),
            capabilities: vec![Capability::SourceFacts],
        },
        provenance: vec![],
        nodes: vec![],
        edges: vec![],
        observations: vec![],
    }
}

struct Fragmented<R>(R);

impl<R: Read> Read for Fragmented<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let length = buffer.len().min(1);
        self.0.read(&mut buffer[..length])
    }
}

#[test]
fn multiple_frames_and_fragmented_reads_preserve_batches() -> Result<(), Box<dyn std::error::Error>>
{
    let codec = FrameCodec::new(MAX_FRAME_BYTES)?;
    let expected = batch();
    let mut bytes = Vec::new();
    codec.write_batch(&mut bytes, &expected)?;
    assert_eq!(&bytes[..8], b"SMEX\0\0\0\x01");
    codec.write_batch(&mut bytes, &expected)?;
    let mut input = Fragmented(Cursor::new(bytes));
    assert_eq!(codec.read_batch(&mut input)?, Some(expected.clone()));
    assert_eq!(codec.read_batch(&mut input)?, Some(expected));
    assert_eq!(codec.read_batch(&mut input)?, None);
    Ok(())
}

#[test]
fn every_truncation_except_clean_eof_is_an_error() -> Result<(), Box<dyn std::error::Error>> {
    let codec = FrameCodec::new(MAX_FRAME_BYTES)?;
    let mut bytes = Vec::new();
    codec.write_batch(&mut bytes, &batch())?;
    assert_eq!(codec.read_batch(&mut &bytes[..0])?, None);
    for length in 1..bytes.len() {
        assert!(matches!(
            codec.read_batch(&mut &bytes[..length]),
            Err(FrameError::Io(_))
        ));
    }
    Ok(())
}

#[test]
fn hostile_headers_are_rejected_without_consuming_payload() -> Result<(), Box<dyn std::error::Error>>
{
    let codec = FrameCodec::new(512)?;
    for (magic, version, length) in [
        (*b"NOPE", WIRE_VERSION, 1_u32),
        (*b"SMEX", 2, 1),
        (*b"SMEX", WIRE_VERSION, 0),
        (*b"SMEX", WIRE_VERSION, 513),
        (*b"SMEX", WIRE_VERSION, u32::MAX),
    ] {
        let mut bytes = magic.to_vec();
        bytes.extend_from_slice(&version.to_be_bytes());
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.push(42);
        let mut reader = Cursor::new(bytes);
        assert!(codec.read_batch(&mut reader).is_err());
        assert_eq!(reader.position(), 12);
    }
    assert!(FrameCodec::new(0).is_err());
    assert!(FrameCodec::new(MAX_FRAME_BYTES + 1).is_err());
    Ok(())
}

#[test]
fn malformed_json_and_invalid_batches_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let codec = FrameCodec::new(MAX_FRAME_BYTES)?;
    let mut invalid = batch();
    invalid.manifest.namespace = "INVALID".to_owned();
    for payload in [b"{} trailing".to_vec(), serde_json::to_vec(&invalid)?] {
        let mut bytes = b"SMEX".to_vec();
        bytes.extend_from_slice(&WIRE_VERSION.to_be_bytes());
        bytes.extend_from_slice(&u32::try_from(payload.len())?.to_be_bytes());
        bytes.extend_from_slice(&payload);
        assert!(codec.read_batch(&mut bytes.as_slice()).is_err());
    }
    let mut output = Vec::new();
    assert!(matches!(
        codec.write_batch(&mut output, &invalid),
        Err(FrameError::InvalidBatch(_))
    ));
    assert!(output.is_empty());
    assert!(matches!(
        FrameCodec::new(1)?.write_batch(&mut output, &batch()),
        Err(FrameError::PayloadLimit)
    ));
    assert!(output.is_empty());
    Ok(())
}

#[test]
fn exact_payload_limit_is_inclusive() -> Result<(), Box<dyn std::error::Error>> {
    let expected = batch();
    let length = serde_json::to_vec(&expected)?.len();
    let codec = FrameCodec::new(length)?;
    let mut bytes = Vec::new();
    codec.write_batch(&mut bytes, &expected)?;
    assert_eq!(codec.read_batch(&mut bytes.as_slice())?, Some(expected));
    assert!(matches!(
        FrameCodec::new(length - 1)?.read_batch(&mut bytes.as_slice()),
        Err(FrameError::PayloadLimit)
    ));
    Ok(())
}

#[test]
fn hand_authored_external_json_uses_existing_sdk_schema() -> Result<(), Box<dyn std::error::Error>>
{
    let json = br#"{"manifest":{"schema_version":1,"namespace":"example.external","producer_version":"1","capabilities":["source_facts"]},"provenance":[],"nodes":[],"edges":[],"observations":[]}"#;
    let mut bytes = b"SMEX\0\0\0\x01".to_vec();
    bytes.extend_from_slice(&u32::try_from(json.len())?.to_be_bytes());
    bytes.extend_from_slice(json);
    assert_eq!(
        FrameCodec::new(MAX_FRAME_BYTES)?.read_batch(&mut bytes.as_slice())?,
        Some(batch())
    );
    Ok(())
}

struct InterruptedOnce<R> {
    reader: R,
    interrupted: bool,
}

#[test]
fn producer_grant_rejects_identity_changes_and_capability_escalation()
-> Result<(), Box<dyn std::error::Error>> {
    let codec = FrameCodec::new(MAX_FRAME_BYTES)?;
    let expected = batch();
    for manifest in [
        ExtensionManifest {
            namespace: "another.producer".to_owned(),
            ..expected.manifest.clone()
        },
        ExtensionManifest {
            producer_version: "2".to_owned(),
            ..expected.manifest.clone()
        },
        ExtensionManifest {
            capabilities: vec![Capability::AnalysisFacts],
            ..expected.manifest.clone()
        },
        ExtensionManifest {
            capabilities: vec![Capability::SourceFacts, Capability::RuntimeObservations],
            ..expected.manifest.clone()
        },
    ] {
        let incoming = FactBatch {
            manifest,
            ..expected.clone()
        };
        let mut bytes = Vec::new();
        codec.write_batch(&mut bytes, &incoming)?;
        assert!(matches!(
            codec.read_batch_for(&mut bytes.as_slice(), &expected.manifest),
            Err(FrameError::UnauthorizedManifest)
        ));
    }
    Ok(())
}

#[test]
fn grants_are_validated_before_reading_and_capability_order_is_irrelevant()
-> Result<(), Box<dyn std::error::Error>> {
    let codec = FrameCodec::new(MAX_FRAME_BYTES)?;
    let mut expected = batch();
    expected
        .manifest
        .capabilities
        .push(Capability::AnalysisFacts);
    let mut bytes = Vec::new();
    codec.write_batch(&mut bytes, &expected)?;
    let mut grant = expected.manifest.clone();
    grant.capabilities.reverse();
    assert_eq!(
        codec.read_batch_for(&mut bytes.as_slice(), &grant)?,
        Some(expected.clone())
    );
    grant.capabilities.push(Capability::RuntimeObservations);
    assert_eq!(
        codec.read_batch_for(&mut bytes.as_slice(), &grant)?,
        Some(expected)
    );
    assert_eq!(codec.read_batch_for(&mut &[][..], &grant)?, None);
    grant.schema_version = 2;
    let mut input = Cursor::new(bytes);
    assert!(matches!(
        codec.read_batch_for(&mut input, &grant),
        Err(FrameError::InvalidBatch(_))
    ));
    assert_eq!(input.position(), 0);
    Ok(())
}

impl<R: Read> Read for InterruptedOnce<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        self.reader.read(bytes)
    }
}

#[test]
fn interrupted_first_read_is_retried_and_write_failure_is_reported()
-> Result<(), Box<dyn std::error::Error>> {
    let codec = FrameCodec::new(MAX_FRAME_BYTES)?;
    let expected = batch();
    let mut bytes = Vec::new();
    codec.write_batch(&mut bytes, &expected)?;
    let mut reader = InterruptedOnce {
        reader: bytes.as_slice(),
        interrupted: false,
    };
    assert_eq!(codec.read_batch(&mut reader)?, Some(expected.clone()));
    let mut short_output = [0_u8; 5];
    assert!(matches!(
        codec.write_batch(&mut short_output.as_mut_slice(), &expected),
        Err(FrameError::Io(_))
    ));
    Ok(())
}
