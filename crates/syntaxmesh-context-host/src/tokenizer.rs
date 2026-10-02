use std::sync::{Arc, Mutex};
use syntaxmesh_query::ContextTokenCounter;
use tiktoken_rs::CoreBPE;
#[cfg(test)]
mod tests;
mod token_count;

#[derive(Clone)]
pub struct ExactTiktokenCounter {
    identity: String,
    encoding: Arc<CoreBPE>,
    memo: Arc<Mutex<token_count::Memo>>,
}

impl ExactTiktokenCounter {
    /// Initialize a supported exact tokenizer.
    ///
    /// # Errors
    /// Rejects unsupported encodings and tokenizer initialization failures.
    pub fn open(name: &str) -> Result<Self, String> {
        let (identity, encoding) = match name {
            "cl100k_base" => (
                "tiktoken-rs-0.12.1/cl100k_base/ordinary",
                tiktoken_rs::cl100k_base(),
            ),
            "o200k_base" => (
                "tiktoken-rs-0.12.1/o200k_base/ordinary",
                tiktoken_rs::o200k_base(),
            ),
            _ => {
                return Err(format!(
                    "unsupported tokenizer {name:?}; choose cl100k_base or o200k_base"
                ));
            }
        };
        let encoding = encoding.map_err(|error| format!("initialize {name} tokenizer: {error}"))?;
        Ok(Self {
            identity: identity.to_owned(),
            encoding: Arc::new(encoding),
            memo: Arc::new(Mutex::new(token_count::Memo::default())),
        })
    }
}

impl ContextTokenCounter for ExactTiktokenCounter {
    fn tokenizer_id(&self) -> &str {
        &self.identity
    }

    fn count_tokens(&self, serialized_pack: &str) -> Result<u64, String> {
        let counted = self
            .memo
            .lock()
            .map_err(|_error| "context token memo lock is poisoned".to_owned())?
            .count(&self.encoding, serialized_pack);
        u64::try_from(counted).map_err(|error| format!("token count exceeds u64: {error}"))
    }
}

impl ExactTiktokenCounter {
    /// Share the initialized encoding with a fresh request-local memo.
    #[must_use]
    pub fn for_request(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            encoding: Arc::clone(&self.encoding),
            memo: Arc::new(Mutex::new(token_count::Memo::default())),
        }
    }
}
