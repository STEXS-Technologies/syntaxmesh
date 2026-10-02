//! Framed external-producer transport; no process, runtime, or store ownership.

mod codec;

pub use codec::{FrameCodec, FrameError, MAX_FRAME_BYTES, WIRE_VERSION};
