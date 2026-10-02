use std::sync::Arc;

use syntaxmesh_query::ContextTokenCounter;

use super::ExactTiktokenCounter;

#[test]
fn request_counter_shares_encoding_not_memo() -> Result<(), Box<dyn std::error::Error>> {
    for name in ["cl100k_base", "o200k_base"] {
        let parent = ExactTiktokenCounter::open(name)?;
        let request = parent.for_request();
        if !Arc::ptr_eq(&parent.encoding, &request.encoding)
            || Arc::ptr_eq(&parent.memo, &request.memo)
            || parent.tokenizer_id() != request.tokenizer_id()
        {
            return Err("request counter did not isolate memo and share encoding".into());
        }
        let payload = "λ 中文 🦀 <|endoftext|> fn main() {}";
        if request.count_tokens(payload)?
            != u64::try_from(parent.encoding.encode_ordinary(payload).len())?
        {
            return Err("request counter differs from full encoding".into());
        }
    }
    if ExactTiktokenCounter::open("unsupported").is_ok() {
        return Err("unsupported encoding accepted".into());
    }
    Ok(())
}
