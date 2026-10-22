use crate::{InferenceError, Limits, Rule, Selection};
use std::io::{Error, ErrorKind, Write};

struct CountingWriter {
    remaining: usize,
}

pub(crate) fn validate_encoded<T: serde::Serialize + ?Sized>(
    value: &T,
    byte_budget: usize,
) -> Result<(), InferenceError> {
    if byte_budget == 0 {
        return Err(InferenceError::InvalidInput);
    }
    serde_json::to_writer(
        CountingWriter {
            remaining: byte_budget,
        },
        value,
    )
    .map_err(|_encoding| InferenceError::BudgetExhausted)
}

impl Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> Result<usize, Error> {
        self.remaining = self.remaining.checked_sub(bytes.len()).ok_or_else(|| {
            Error::new(
                ErrorKind::OutOfMemory,
                "inference input byte budget exhausted",
            )
        })?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

pub(crate) fn validate(
    selection: &Selection<'_>,
    rules: &[Rule],
    limits: &Limits,
) -> Result<(), InferenceError> {
    if limits.input_bytes == 0 || limits.facts == 0 {
        return Err(InferenceError::InvalidInput);
    }
    if selection
        .nodes
        .len()
        .checked_add(selection.premises.len())
        .is_none_or(|count| count > limits.facts)
    {
        return Err(InferenceError::BudgetExhausted);
    }
    validate_encoded(
        &(
            selection.repository,
            selection.worktree,
            selection.generation,
            selection.nodes,
            selection.premises,
            rules,
        ),
        limits.input_bytes,
    )
}

#[cfg(test)]
mod tests;
