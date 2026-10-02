use syntaxmesh_semantic::SemanticOutput;

const MAX_SPLIT_DEPTH: usize = 3;

pub(super) enum PromptError {
    Truncated,
    Failed(String),
}

impl From<String> for PromptError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

pub(super) fn extract<T>(
    requests: &[T],
    execute: &impl Fn(&[T]) -> Result<SemanticOutput, PromptError>,
) -> Result<SemanticOutput, String> {
    let mut output = SemanticOutput::default();
    for (_, result) in groups_at_depth(requests, execute, 0) {
        output.claims.extend(result?.claims);
    }
    Ok(output)
}

pub(super) fn extract_many<T>(
    requests: &[T],
    execute: &impl Fn(&[T]) -> Result<Vec<SemanticOutput>, PromptError>,
) -> Vec<Result<SemanticOutput, String>> {
    let mut results = Vec::with_capacity(requests.len());
    for (count, result) in groups_at_depth(requests, execute, 0) {
        match result {
            Ok(outputs) if outputs.len() == count => results.extend(outputs.into_iter().map(Ok)),
            Ok(_) => results.extend((0..count).map(|_| {
                Err("semantic recovery returned incorrect document result count".to_owned())
            })),
            Err(error) => results.extend((0..count).map(|_| Err(error.clone()))),
        }
    }
    results
}

fn groups_at_depth<T, O>(
    requests: &[T],
    execute: &impl Fn(&[T]) -> Result<O, PromptError>,
    depth: usize,
) -> Vec<(usize, Result<O, String>)> {
    match execute(requests) {
        Ok(output) => vec![(requests.len(), Ok(output))],
        Err(PromptError::Failed(message)) => vec![(requests.len(), Err(message))],
        Err(PromptError::Truncated) if requests.len() > 1 && depth < MAX_SPLIT_DEPTH => {
            let (left, right) = requests.split_at(requests.len() / 2);
            let next_depth = depth.saturating_add(1);
            let mut completed = groups_at_depth(left, execute, next_depth);
            completed.extend(groups_at_depth(right, execute, next_depth));
            completed
        }
        Err(PromptError::Truncated) => vec![(requests.len(), Err(
            "semantic provider truncated the structured response; bounded prompt splitting exhausted or request cannot be split".to_owned(),
        ))],
    }
}

#[cfg(test)]
mod tests;
