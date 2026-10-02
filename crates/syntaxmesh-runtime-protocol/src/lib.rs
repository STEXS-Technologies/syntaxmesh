//! Public, lightweight runtime-observation protocol.

mod observation;

pub use observation::{
    OBSERVATION_SCHEMA_VERSION, Observation, ObservationError, ObservationOrigin,
};
