use super::*;

#[cfg(test)]
mod tests;

pub(super) struct Declarations {
    pub(super) nodes: Vec<Node>,
    occurrences: BTreeMap<(String, String), usize>,
}

impl Declarations {
    pub(super) fn into_nodes(self) -> Vec<Node> {
        self.nodes
    }

    pub(super) fn new(file_module: Node) -> Self {
        let mut occurrences = BTreeMap::new();
        occurrences.insert(
            (format!("{:?}", file_module.kind), file_module.name.clone()),
            1,
        );
        Self {
            nodes: vec![file_module],
            occurrences,
        }
    }

    pub(super) fn identity(
        &mut self,
        kind: &NodeKind,
        name: &str,
    ) -> Result<String, ExtractorError> {
        let ordinal = self
            .occurrences
            .entry((format!("{kind:?}"), name.to_owned()))
            .or_default();
        let identity = if *ordinal == 0 {
            name.to_owned()
        } else {
            format!("{name}#{ordinal}")
        };
        *ordinal = ordinal.checked_add(1).ok_or_else(|| {
            ExtractorError::InvalidInput("declaration ordinal overflow".to_owned())
        })?;
        Ok(identity)
    }
}
