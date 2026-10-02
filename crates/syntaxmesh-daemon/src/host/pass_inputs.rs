/// Capture sources before loading the policy used to prepare that inventory.
pub(super) fn capture<T, U>(
    sources: impl FnOnce() -> Result<T, String>,
    policy: impl FnOnce() -> Result<U, String>,
) -> Result<(T, U), String> {
    let files = sources()?;
    let configuration = policy()?;
    Ok((files, configuration))
}

#[cfg(test)]
mod tests;
