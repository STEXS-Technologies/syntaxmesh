use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use syntaxmesh_api_model::{
    INFERENCE_READ_SCHEMA_VERSION, InferenceCheckpointRequest, InferencePageLimits,
    InferencePageRequest,
};
use syntaxmesh_core::{GenerationId, RepositoryId, WorktreeId};
use syntaxmesh_inference_sqlite::{
    ContractHistoryCursor, ContractHistoryQuery, StreamCursor, StreamOrigin, StreamPageRequest,
};

#[derive(Clone)]
struct Binding {
    database: PathBuf,
    origin: StreamOrigin,
}

/// Operator-owned configuration. Admission performs no filesystem access.
#[derive(Clone)]
pub struct InferenceReadBindings {
    repository: RepositoryId,
    worktree: WorktreeId,
    limits: InferencePageLimits,
    bindings: BTreeMap<String, Binding>,
}

/// Storage-aware host-local request, not a remote filesystem capability.
pub struct AdmittedInferencePage<'bindings> {
    pub database: &'bindings Path,
    pub request: StreamPageRequest,
}

/// Storage-aware, operator-scoped retained contract history request.
pub struct AdmittedContractHistory<'bindings> {
    pub database: &'bindings Path,
    pub query: ContractHistoryQuery,
}

impl InferenceReadBindings {
    /// Immutable repository/worktree scope selected by the operator.
    #[must_use]
    pub const fn scope(&self) -> (RepositoryId, WorktreeId) {
        (self.repository, self.worktree)
    }

    /// Create a scoped registry with positive, platform-representable ceilings.
    ///
    /// # Errors
    /// Rejects zero or unrepresentable policy limits.
    pub fn new(
        repository: RepositoryId,
        worktree: WorktreeId,
        limits: InferencePageLimits,
    ) -> Result<Self, String> {
        admit_limits(limits, limits)?;
        Ok(Self {
            repository,
            worktree,
            limits,
            bindings: BTreeMap::new(),
        })
    }

    /// Install trusted operator configuration without opening or creating storage.
    ///
    /// # Errors
    /// Rejects invalid/duplicate identifiers, foreign scope and registry overflow.
    pub fn insert(
        &mut self,
        identifier: String,
        database: PathBuf,
        origin: StreamOrigin,
    ) -> Result<(), String> {
        valid_identifier(&identifier)?;
        let selection = origin.baseline.selection;
        if selection.repository != self.repository || selection.worktree != self.worktree {
            return Err("inference binding outside host scope".to_owned());
        }
        if self.bindings.contains_key(&identifier) || self.bindings.len() >= 256 {
            return Err("duplicate inference binding or registry limit".to_owned());
        }
        self.bindings
            .insert(identifier, Binding { database, origin });
        Ok(())
    }

    /// Resolve one explicit retained generation without opening storage.
    ///
    /// # Errors
    /// Rejects unsupported versions and unknown/invalid binding identifiers.
    pub fn checkpoint(
        &self,
        request: &InferenceCheckpointRequest,
    ) -> Result<(&Path, StreamOrigin, GenerationId), String> {
        let binding = self.binding(request.schema_version, &request.binding)?;
        Ok((&binding.database, binding.origin, request.generation))
    }

    /// Admit independent page bounds and bind every cursor coordinate.
    ///
    /// # Errors
    /// Rejects invalid version/binding, limits and cursor scope substitutions.
    /// Retained root/key validity is subsequently checked by the materializer.
    pub fn page(
        &self,
        request: &InferencePageRequest,
    ) -> Result<AdmittedInferencePage<'_>, String> {
        let binding = self.binding(request.schema_version, &request.binding)?;
        let [claim_budget, byte_budget, premise_budget, page_budget] =
            admit_limits(request.limits, self.limits)?;
        let cursor = request
            .cursor
            .as_ref()
            .map(|cursor| {
                let selection = cursor.selection;
                if cursor.binding != request.binding
                    || cursor.stream != binding.origin.stream
                    || selection.repository != self.repository
                    || selection.worktree != self.worktree
                    || selection.generation != request.generation
                {
                    return Err("inference cursor binding or selection substitution".to_owned());
                }
                Ok(StreamCursor {
                    stream: cursor.stream,
                    selection: syntaxmesh_inference::ClaimSelection {
                        repository: selection.repository,
                        worktree: selection.worktree,
                        generation: selection.generation,
                    },
                    root: cursor.root,
                    after: cursor.after,
                })
            })
            .transpose()?;
        Ok(AdmittedInferencePage {
            database: &binding.database,
            request: StreamPageRequest {
                origin: binding.origin,
                generation: request.generation,
                cursor,
                claim_budget,
                byte_budget,
                premise_budget,
                page_budget,
            },
        })
    }

    /// Bind retained contract history to one installed operator database and scope.
    ///
    /// # Errors
    /// Rejects unsupported schema, unknown binding, invalid identifiers, bounds,
    /// or a cursor that attempts to substitute host scope or contract identity.
    pub fn contract_history(
        &self,
        schema_version: u32,
        identifier: &str,
        contract_id: String,
        contract_version: String,
        after: Option<ContractHistoryCursor>,
        limit: usize,
    ) -> Result<AdmittedContractHistory<'_>, String> {
        let binding = self.binding(schema_version, identifier)?;
        valid_contract_identity(&contract_id, &contract_version)?;
        if !(1..=100).contains(&limit) {
            return Err("contract history page limit outside host policy".to_owned());
        }
        if after.as_ref().is_some_and(|cursor| {
            cursor.repository != self.repository
                || cursor.worktree != self.worktree
                || cursor.contract_id != contract_id
                || cursor.contract_version != contract_version
        }) {
            return Err("contract history cursor scope substitution".to_owned());
        }
        Ok(AdmittedContractHistory {
            database: &binding.database,
            query: ContractHistoryQuery {
                repository: self.repository,
                worktree: self.worktree,
                contract_id,
                contract_version,
                after,
                limit,
            },
        })
    }

    fn binding(&self, version: u32, identifier: &str) -> Result<&Binding, String> {
        if version != INFERENCE_READ_SCHEMA_VERSION {
            return Err("unsupported inference read schema".to_owned());
        }
        valid_identifier(identifier)?;
        self.bindings
            .get(identifier)
            .ok_or_else(|| "unknown inference binding".to_owned())
    }
}

fn valid_contract_identity(id: &str, version: &str) -> Result<(), String> {
    if id.trim().is_empty() || id.len() > 128 || version.trim().is_empty() || version.len() > 64 {
        return Err("invalid contract identity".to_owned());
    }
    Ok(())
}

pub(crate) fn valid_identifier(identifier: &str) -> Result<(), String> {
    if identifier.is_empty()
        || identifier.len() > 128
        || !identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("invalid inference binding identifier".to_owned());
    }
    Ok(())
}

fn admit_limits(
    request: InferencePageLimits,
    ceiling: InferencePageLimits,
) -> Result<[usize; 4], String> {
    let mut admitted = [0; 4];
    for ((value, maximum), destination) in [
        (request.claims, ceiling.claims),
        (request.payload_bytes, ceiling.payload_bytes),
        (request.premises, ceiling.premises),
        (request.tree_pages, ceiling.tree_pages),
    ]
    .into_iter()
    .zip(admitted.iter_mut())
    {
        if value == 0 || value > maximum {
            return Err("inference page limit outside operator policy".to_owned());
        }
        *destination = usize::try_from(value)
            .map_err(|_error| "inference page limit exceeds platform".to_owned())?;
    }
    Ok(admitted)
}

#[cfg(test)]
mod tests;
