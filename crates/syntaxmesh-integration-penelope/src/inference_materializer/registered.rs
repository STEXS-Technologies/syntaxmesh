use super::{
    PenelopeInferenceMaterializer, PenelopeInferenceSetup, PenelopeInferenceStream,
    StreamBeginRequest, failure, journal,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use syntaxmesh_core::StableId;
pub use syntaxmesh_inference::RegisteredInferenceAdmission;
use syntaxmesh_inference::{
    ClaimSelection, InvalidationLimits, Limits, OntologyDefinition, OntologyRegistry, Rule,
    materialization_identity, rule_set_identity,
};
use syntaxmesh_inference_sqlite::{
    InferenceMaterializer, RebuildRequest, StreamCheckpoint, StreamOrigin, StreamSyncRequest,
    Watermark,
};
use syntaxmesh_store::{DurableRecordStore, GraphStore, StoreError};
use syntaxmesh_workflow::WorkflowError;

const PREFIX: &str = "syntaxmesh.penelope.inference-registered.v1/";
const STREAM_PREFIX: &str = "syntaxmesh.penelope.inference-registered-stream.v1/";
const SETUP_PREFIX: &str = "syntaxmesh.penelope.inference-registered-setup.v1/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Request {
    selection: ClaimSelection,
    operation: StableId,
    database: String,
    rules: Vec<Rule>,
    definitions: Vec<OntologyDefinition>,
    registry_entry_budget: usize,
    registry_byte_budget: usize,
    binding_budget: usize,
    limits: Limits,
    history_scan_budget: usize,
    rule_budget: usize,
    output_bytes: usize,
    premise_budget: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RegisteredSetupRequest {
    rebuild: Request,
    page_budget: usize,
}

impl journal::Plan for RegisteredSetupRequest {
    type Receipt = StreamOrigin;
    const DEFINITION: &'static str = "def_syntaxmesh_registered_inference_setup";
    const STEP: &'static str = "registered_inference_setup";

    fn key(&self) -> Result<String, WorkflowError> {
        let child_key = journal::key(&self.rebuild)?;
        let suffix = child_key
            .rsplit('/')
            .next()
            .ok_or_else(|| failure("missing registered setup identity"))?;
        Ok(format!("{SETUP_PREFIX}{suffix}"))
    }

    fn validate_receipt(&self, receipt: StreamOrigin) -> Result<(), WorkflowError> {
        if self.page_budget == 0
            || receipt.baseline.operation != self.rebuild.operation
            || receipt.baseline.selection != self.rebuild.selection
            || receipt.baseline.claim_count > self.rebuild.limits.claims
            || receipt.stream
                != syntaxmesh_core::StableId::derive(
                    "inference-stream-v1",
                    &[&receipt.baseline.operation.0],
                )
        {
            return Err(failure(
                "registered setup receipt identity or admission mismatch",
            ));
        }
        Ok(())
    }
}

impl Request {
    fn registry(&self) -> Result<OntologyRegistry, WorkflowError> {
        OntologyRegistry::from_definitions(
            &self.definitions,
            self.registry_entry_budget,
            self.registry_byte_budget,
        )
        .map_err(failure)
    }

    fn validate_identity(&self) -> Result<(), WorkflowError> {
        let registry = self.registry()?;
        let bound = registry
            .bind_rules(
                &self.rules,
                self.rule_budget,
                self.binding_budget,
                self.limits.input_bytes,
            )
            .map_err(failure)?;
        if materialization_identity(
            self.selection,
            bound.rules(),
            self.rule_budget,
            self.limits.input_bytes,
        )
        .map_err(failure)?
            != self.operation
        {
            return Err(failure("registered inference operation mismatch"));
        }
        Ok(())
    }
}

fn canonical_definitions(definitions: &[OntologyDefinition]) -> Vec<OntologyDefinition> {
    let mut definitions = definitions.to_vec();
    for definition in &mut definitions {
        definition.concepts.sort_unstable();
        for relation in &mut definition.relations {
            relation.subject_domain.sort_unstable();
            relation.object_range.sort_unstable();
        }
        definition
            .relations
            .sort_by(|left, right| left.name.cmp(&right.name));
    }
    definitions.sort_by(|left, right| {
        (&left.namespace, &left.version).cmp(&(&right.namespace, &right.version))
    });
    definitions
}

fn make_registered_request<S: GraphStore>(
    store: &S,
    database: impl AsRef<Path>,
    request: &RebuildRequest<'_>,
    definitions: &[OntologyDefinition],
    admission: RegisteredInferenceAdmission,
) -> Result<Request, WorkflowError> {
    if request.history_scan_budget == 0
        || request.output_bytes == 0
        || request.premise_budget == 0
        || request.limits.facts == 0
        || request.limits.edge_visits == 0
        || request.limits.depth == 0
        || request.limits.claims == 0
        || admission.registry_entries == 0
        || admission.registry_bytes == 0
        || admission.bindings == 0
    {
        return Err(failure("positive registered inference admission required"));
    }
    let definitions = canonical_definitions(definitions);
    let registry = OntologyRegistry::from_definitions(
        &definitions,
        admission.registry_entries,
        admission.registry_bytes,
    )
    .map_err(failure)?;
    let bound = registry
        .bind_rules(
            request.rules,
            request.rule_budget,
            admission.bindings,
            request.limits.input_bytes,
        )
        .map_err(failure)?;
    let manifest = store.manifest(request.generation)?;
    let selection = ClaimSelection {
        repository: manifest.repository,
        worktree: manifest.worktree,
        generation: request.generation,
    };
    let operation = materialization_identity(
        selection,
        bound.rules(),
        request.rule_budget,
        request.limits.input_bytes,
    )
    .map_err(failure)?;
    let path = database.as_ref();
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(failure)?.join(path)
    };
    let database_path = absolute
        .into_os_string()
        .into_string()
        .map_err(|_path| failure("registered inference database path must be UTF-8"))?;
    let mut rules = request.rules.to_vec();
    rules.sort_by(|left, right| (&left.id, &left.version).cmp(&(&right.id, &right.version)));
    Ok(Request {
        selection,
        operation,
        database: database_path,
        rules,
        definitions,
        registry_entry_budget: admission.registry_entries,
        registry_byte_budget: admission.registry_bytes,
        binding_budget: admission.bindings,
        limits: *request.limits,
        history_scan_budget: request.history_scan_budget,
        rule_budget: request.rule_budget,
        output_bytes: request.output_bytes,
        premise_budget: request.premise_budget,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RegisteredStreamRequest {
    origin: StreamOrigin,
    through: syntaxmesh_core::GenerationId,
    sequence: u64,
    database: String,
    rules: Vec<Rule>,
    definitions: Vec<OntologyDefinition>,
    registry_entry_budget: usize,
    registry_byte_budget: usize,
    binding_budget: usize,
    limits: Limits,
    range_limits: InvalidationLimits,
    history_scan_budget: usize,
    rule_budget: usize,
    output_bytes: usize,
    premise_budget: usize,
    evidence_budget: usize,
    matched_claim_bytes: usize,
    page_budget: usize,
}

impl RegisteredStreamRequest {
    fn registry(&self) -> Result<OntologyRegistry, WorkflowError> {
        OntologyRegistry::from_definitions(
            &self.definitions,
            self.registry_entry_budget,
            self.registry_byte_budget,
        )
        .map_err(failure)
    }

    fn bound_rules(&self) -> Result<syntaxmesh_inference::BoundRuleSet, WorkflowError> {
        self.registry()?
            .bind_rules(
                &self.rules,
                self.rule_budget,
                self.binding_budget,
                self.limits.input_bytes,
            )
            .map_err(failure)
    }

    fn borrowed(&self) -> StreamSyncRequest<'_> {
        StreamSyncRequest {
            origin: self.origin,
            through: self.through,
            rules: &self.rules,
            limits: &self.limits,
            range_limits: &self.range_limits,
            history_scan_budget: self.history_scan_budget,
            rule_budget: self.rule_budget,
            output_bytes: self.output_bytes,
            premise_budget: self.premise_budget,
            evidence_budget: self.evidence_budget,
            matched_claim_bytes: self.matched_claim_bytes,
            page_budget: self.page_budget,
        }
    }
}

impl journal::Plan for RegisteredStreamRequest {
    type Receipt = StreamCheckpoint;
    const DEFINITION: &'static str = "def_syntaxmesh_registered_inference_stream";
    const STEP: &'static str = "registered_inference_stream";

    fn key(&self) -> Result<String, WorkflowError> {
        let bound = self.bound_rules()?;
        if rule_set_identity(bound.rules(), self.rule_budget, self.limits.input_bytes)
            .map_err(failure)?
            != self.origin.baseline.configuration
        {
            return Err(failure("registered stream baseline configuration mismatch"));
        }
        let encoded =
            bincode::serialize(&(self.origin, self.through, self.sequence, &self.database))
                .map_err(failure)?;
        Ok(format!(
            "{STREAM_PREFIX}{:020}/{}",
            self.sequence,
            hex::encode(penelope::ContentDigest::sha256(&encoded).0)
        ))
    }

    fn validate_receipt(&self, receipt: StreamCheckpoint) -> Result<(), WorkflowError> {
        let selection = self.origin.baseline.selection;
        if receipt.stream != self.origin.stream
            || receipt.selection.generation != self.through
            || receipt.selection.repository != selection.repository
            || receipt.selection.worktree != selection.worktree
            || receipt.sequence != self.sequence
            || receipt.added_claims > self.limits.claims
            || receipt.affected_claims > self.range_limits.claims
            || receipt.closure_reasons > self.evidence_budget
        {
            return Err(failure(
                "registered stream receipt scope or admission mismatch",
            ));
        }
        Ok(())
    }
}

impl journal::Plan for Request {
    type Receipt = Watermark;
    const DEFINITION: &'static str = "def_syntaxmesh_registered_inference";
    const STEP: &'static str = "registered_inference";

    fn key(&self) -> Result<String, WorkflowError> {
        self.validate_identity()?;
        let bytes = bincode::serialize(&(self.operation, &self.database)).map_err(failure)?;
        Ok(format!(
            "{PREFIX}{}",
            hex::encode(penelope::ContentDigest::sha256(&bytes).0)
        ))
    }

    fn validate_receipt(&self, receipt: Watermark) -> Result<(), WorkflowError> {
        if receipt.operation != self.operation || receipt.selection != self.selection {
            return Err(failure("registered inference receipt scope mismatch"));
        }
        Ok(())
    }
}

impl<S: GraphStore + DurableRecordStore> PenelopeInferenceMaterializer<S> {
    /// Journal and atomically publish a complete ontology-validated inference rebuild.
    /// The exact definitions and admission bounds are retained for restart recovery.
    ///
    /// # Errors
    /// Rejects invalid schemas, changed requests, corrupt journals and failed effects.
    pub fn rebuild_registered(
        &mut self,
        database: impl AsRef<Path>,
        request: &RebuildRequest<'_>,
        definitions: &[OntologyDefinition],
        admission: RegisteredInferenceAdmission,
    ) -> Result<Watermark, WorkflowError> {
        let registered =
            make_registered_request(&self.store, database, request, definitions, admission)?;
        self.run_registered(&registered)
    }

    /// Recover prepared registered rebuilds from their versioned journal records.
    ///
    /// # Errors
    /// Rejects malformed records/pages or unavailable source/effect state.
    pub fn recover_registered_pending(&mut self) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = 0_usize;
        loop {
            let page = self
                .store
                .records_with_prefix_page(PREFIX, cursor.as_deref(), 32)?;
            if page.records.len() > 32 {
                return Err(failure("oversized registered inference recovery page"));
            }
            let mut previous = cursor.clone();
            for (key, bytes) in &page.records {
                if !key.starts_with(PREFIX) || previous.as_ref().is_some_and(|last| key <= last) {
                    return Err(failure("unordered registered inference recovery key"));
                }
                let record = journal::decode::<Request>(bytes)?;
                if journal::key(&record.request)? != *key {
                    return Err(failure("registered inference journal key substitution"));
                }
                journal::validate(&record, &record.request)?;
                if record.receipt.is_none() {
                    self.run_registered(&record.request)?;
                    recovered = recovered
                        .checked_add(1)
                        .ok_or_else(|| failure("registered recovery count overflow"))?;
                }
                previous = Some(key.clone());
            }
            let Some(next) = page.next_cursor else {
                return Ok(recovered);
            };
            if previous.as_deref() != Some(next.as_str())
                || cursor.as_ref().is_some_and(|last| &next <= last)
            {
                return Err(failure("nonadvancing registered recovery cursor"));
            }
            cursor = Some(next);
        }
    }

    fn run_registered(&mut self, request: &Request) -> Result<Watermark, WorkflowError> {
        let key = journal::key(request)?;
        let (record, bytes) = match self.store.read_record(&key)? {
            Some(bytes) => {
                let record = journal::decode(&bytes)?;
                journal::validate(&record, request)?;
                (record, bytes)
            }
            None => {
                let record = journal::prepare(request.clone())?;
                let bytes = journal::encode(&record)?;
                match self.store.compare_exchange_record(&key, None, &bytes) {
                    Ok(()) => (record, bytes),
                    Err(StoreError::RecordConflict(_)) => {
                        let raced = self.store.read_record(&key)?.ok_or_else(|| {
                            failure("registered inference journal disappeared during prepare")
                        })?;
                        let raced_record = journal::decode(&raced)?;
                        journal::validate(&raced_record, request)?;
                        (raced_record, raced)
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        };
        if let Some(receipt) = record.receipt {
            self.validate_registered_effect(request, receipt)?;
            return Ok(receipt);
        }
        let manifest = self.store.manifest(request.selection.generation)?;
        if manifest.repository != request.selection.repository
            || manifest.worktree != request.selection.worktree
        {
            return Err(failure("registered inference source scope mismatch"));
        }
        let registry = request.registry()?;
        let limits = request.limits;
        let rebuild = RebuildRequest {
            generation: request.selection.generation,
            rules: &request.rules,
            limits: &limits,
            history_scan_budget: request.history_scan_budget,
            rule_budget: request.rule_budget,
            output_bytes: request.output_bytes,
            premise_budget: request.premise_budget,
        };
        let receipt = InferenceMaterializer::open(&request.database)
            .map_err(failure)?
            .rebuild_registered(&self.store, &rebuild, &registry, request.binding_budget)
            .map_err(failure)?;
        let replacement = journal::encode(&journal::complete(record, receipt)?)?;
        match self
            .store
            .compare_exchange_record(&key, Some(&bytes), &replacement)
        {
            Ok(()) => Ok(receipt),
            Err(StoreError::RecordConflict(_)) => {
                let current = self
                    .store
                    .read_record(&key)?
                    .ok_or_else(|| failure("registered inference journal disappeared"))?;
                let current = journal::decode(&current)?;
                journal::validate(&current, request)?;
                if current.receipt != Some(receipt) {
                    return Err(failure("racing registered inference completion mismatch"));
                }
                self.validate_registered_effect(request, receipt)?;
                Ok(receipt)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn validate_registered_effect(
        &self,
        request: &Request,
        receipt: Watermark,
    ) -> Result<(), WorkflowError> {
        <Request as journal::Plan>::validate_receipt(request, receipt)?;
        InferenceMaterializer::open(&request.database)
            .map_err(failure)?
            .claims(receipt, request.limits.claims, request.output_bytes)
            .map_err(failure)?;
        Ok(())
    }
}

impl<S: GraphStore + DurableRecordStore> super::PenelopeInferenceStream<S> {
    /// Journal and synchronize one registered immediate successor.
    ///
    /// The request retains complete definitions for recovery. Membership
    /// retractions use bounded target-selection reconciliation.
    ///
    /// # Errors
    /// Rejects changed configuration, invalid ontology facts, malformed journals,
    /// skipped transitions, exhausted reconciliation bounds or failed effects.
    pub fn sync_registered(
        &mut self,
        database: impl AsRef<Path>,
        request: &StreamSyncRequest<'_>,
        definitions: &[OntologyDefinition],
        admission: RegisteredInferenceAdmission,
    ) -> Result<StreamCheckpoint, WorkflowError> {
        if request.page_budget == 0
            || request.output_bytes == 0
            || request.premise_budget == 0
            || request.evidence_budget == 0
            || request.matched_claim_bytes == 0
            || request.history_scan_budget == 0
            || request.range_limits.generations == 0
            || request.range_limits.versions == 0
            || request.range_limits.postings == 0
            || request.range_limits.claims == 0
            || request.limits.facts == 0
            || request.limits.edge_visits == 0
            || request.limits.depth == 0
            || request.limits.claims == 0
            || admission.registry_entries == 0
            || admission.registry_bytes == 0
            || admission.bindings == 0
        {
            return Err(failure("positive registered stream admission required"));
        }
        let definitions = canonical_definitions(definitions);
        let registry = OntologyRegistry::from_definitions(
            &definitions,
            admission.registry_entries,
            admission.registry_bytes,
        )
        .map_err(failure)?;
        let bound = registry
            .bind_rules(
                request.rules,
                request.rule_budget,
                admission.bindings,
                request.limits.input_bytes,
            )
            .map_err(failure)?;
        if rule_set_identity(
            bound.rules(),
            request.rule_budget,
            request.limits.input_bytes,
        )
        .map_err(failure)?
            != request.origin.baseline.configuration
        {
            return Err(failure("registered stream baseline configuration mismatch"));
        }
        let path = database.as_ref();
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map_err(failure)?.join(path)
        };
        let database_path = absolute
            .into_os_string()
            .into_string()
            .map_err(|_path| failure("registered stream database path must be UTF-8"))?;
        let mut rules = request.rules.to_vec();
        rules.sort_by(|left, right| (&left.id, &left.version).cmp(&(&right.id, &right.version)));
        self.run_registered_stream(&RegisteredStreamRequest {
            origin: request.origin,
            through: request.through,
            sequence: self.store.generation_sequence(request.through)?,
            database: database_path,
            rules,
            definitions,
            registry_entry_budget: admission.registry_entries,
            registry_byte_budget: admission.registry_bytes,
            binding_budget: admission.bindings,
            limits: *request.limits,
            range_limits: *request.range_limits,
            history_scan_budget: request.history_scan_budget,
            rule_budget: request.rule_budget,
            output_bytes: request.output_bytes,
            premise_budget: request.premise_budget,
            evidence_budget: request.evidence_budget,
            matched_claim_bytes: request.matched_claim_bytes,
            page_budget: request.page_budget,
        })
    }

    /// Recover prepared registered successors in canonical sequence order.
    ///
    /// # Errors
    /// Rejects malformed records/pages or unavailable source/effect state.
    pub fn recover_registered_stream_pending(&mut self) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = 0_usize;
        loop {
            let page = self
                .store
                .records_with_prefix_page(STREAM_PREFIX, cursor.as_deref(), 32)?;
            if page.records.len() > 32 {
                return Err(failure("oversized registered stream recovery page"));
            }
            let mut previous = cursor.clone();
            for (key, bytes) in &page.records {
                if !key.starts_with(STREAM_PREFIX)
                    || previous.as_ref().is_some_and(|last| key <= last)
                {
                    return Err(failure("unordered registered stream recovery key"));
                }
                let record = journal::decode::<RegisteredStreamRequest>(bytes)?;
                if journal::key(&record.request)? != *key {
                    return Err(failure("registered stream journal key substitution"));
                }
                journal::validate(&record, &record.request)?;
                if record.receipt.is_none() {
                    self.run_registered_stream(&record.request)?;
                    recovered = recovered
                        .checked_add(1)
                        .ok_or_else(|| failure("registered stream recovery count overflow"))?;
                }
                previous = Some(key.clone());
            }
            let Some(next) = page.next_cursor else {
                return Ok(recovered);
            };
            if previous.as_deref() != Some(next.as_str())
                || cursor.as_ref().is_some_and(|last| &next <= last)
            {
                return Err(failure("nonadvancing registered stream recovery cursor"));
            }
            cursor = Some(next);
        }
    }

    fn run_registered_stream(
        &mut self,
        request: &RegisteredStreamRequest,
    ) -> Result<StreamCheckpoint, WorkflowError> {
        if self.store.generation_sequence(request.through)? != request.sequence {
            return Err(failure("registered stream canonical sequence mismatch"));
        }
        let key = journal::key(request)?;
        let (record, bytes) = match self.store.read_record(&key)? {
            Some(bytes) => {
                let record = journal::decode(&bytes)?;
                journal::validate(&record, request)?;
                (record, bytes)
            }
            None => {
                let record = journal::prepare(request.clone())?;
                let bytes = journal::encode(&record)?;
                match self.store.compare_exchange_record(&key, None, &bytes) {
                    Ok(()) => (record, bytes),
                    Err(StoreError::RecordConflict(_)) => {
                        let raced = self.store.read_record(&key)?.ok_or_else(|| {
                            failure("registered stream journal disappeared during prepare")
                        })?;
                        let raced_record = journal::decode(&raced)?;
                        journal::validate(&raced_record, request)?;
                        (raced_record, raced)
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        };
        if let Some(receipt) = record.receipt {
            self.validate_registered_stream_effect(request, receipt)?;
            return Ok(receipt);
        }
        let registry = request.registry()?;
        let receipt = InferenceMaterializer::open(&request.database)
            .map_err(failure)?
            .sync_stream_registered(
                &self.store,
                &request.borrowed(),
                &registry,
                request.binding_budget,
            )
            .map_err(failure)?;
        let replacement = journal::encode(&journal::complete(record, receipt)?)?;
        match self
            .store
            .compare_exchange_record(&key, Some(&bytes), &replacement)
        {
            Ok(()) => Ok(receipt),
            Err(StoreError::RecordConflict(_)) => {
                let current = self
                    .store
                    .read_record(&key)?
                    .ok_or_else(|| failure("registered stream journal disappeared"))?;
                let current = journal::decode(&current)?;
                journal::validate(&current, request)?;
                if current.receipt != Some(receipt) {
                    return Err(failure("racing registered stream completion mismatch"));
                }
                self.validate_registered_stream_effect(request, receipt)?;
                Ok(receipt)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn validate_registered_stream_effect(
        &self,
        request: &RegisteredStreamRequest,
        receipt: StreamCheckpoint,
    ) -> Result<(), WorkflowError> {
        <RegisteredStreamRequest as journal::Plan>::validate_receipt(request, receipt)?;
        let registry = request.registry()?;
        let mut materializer = InferenceMaterializer::open(&request.database).map_err(failure)?;
        if materializer
            .stream_checkpoint(request.origin, request.through)
            .map_err(failure)?
            != Some(receipt)
            || materializer
                .sync_stream_registered(
                    &self.store,
                    &request.borrowed(),
                    &registry,
                    request.binding_budget,
                )
                .map_err(failure)?
                != receipt
        {
            return Err(failure(
                "completed registered stream journal has no matching retained effect",
            ));
        }
        Ok(())
    }
}

impl<S: GraphStore + DurableRecordStore> PenelopeInferenceSetup<S> {
    /// Journal an ontology-bound baseline and its stream initialization as one setup.
    ///
    /// # Errors
    /// Rejects invalid bounds, changed requests, corrupt journals and failed effects.
    pub fn initialize_registered(
        &mut self,
        database: impl AsRef<Path>,
        request: &RebuildRequest<'_>,
        definitions: &[OntologyDefinition],
        admission: RegisteredInferenceAdmission,
        page_budget: usize,
    ) -> Result<StreamOrigin, WorkflowError> {
        if page_budget == 0 {
            return Err(failure("positive registered setup page budget required"));
        }
        let rebuild =
            make_registered_request(&self.store, database, request, definitions, admission)?;
        self.run_registered_setup(&RegisteredSetupRequest {
            rebuild,
            page_budget,
        })
    }

    /// Recover registered parent setup intents from bounded, ordered pages.
    ///
    /// # Errors
    /// Rejects malformed records/continuations and unavailable child effects.
    pub fn recover_registered_pending(&mut self) -> Result<usize, WorkflowError> {
        let mut cursor: Option<String> = None;
        let mut recovered = 0_usize;
        loop {
            let page = self
                .store
                .records_with_prefix_page(SETUP_PREFIX, cursor.as_deref(), 32)?;
            if page.records.len() > 32 {
                return Err(failure("oversized registered setup recovery page"));
            }
            let mut previous = cursor.clone();
            for (key, bytes) in &page.records {
                if !key.starts_with(SETUP_PREFIX)
                    || previous.as_ref().is_some_and(|last| key <= last)
                {
                    return Err(failure("unordered registered setup recovery key"));
                }
                let record = journal::decode::<RegisteredSetupRequest>(bytes)?;
                if journal::key(&record.request)? != *key {
                    return Err(failure("registered setup key substitution"));
                }
                journal::validate(&record, &record.request)?;
                if record.receipt.is_none() {
                    self.run_registered_setup(&record.request)?;
                    recovered = recovered
                        .checked_add(1)
                        .ok_or_else(|| failure("registered setup recovery count overflow"))?;
                }
                previous = Some(key.clone());
            }
            let Some(next) = page.next_cursor else {
                return Ok(recovered);
            };
            if previous.as_deref() != Some(next.as_str())
                || cursor.as_ref().is_some_and(|last| &next <= last)
            {
                return Err(failure("nonadvancing registered setup recovery cursor"));
            }
            cursor = Some(next);
        }
    }

    fn run_registered_setup(
        &mut self,
        request: &RegisteredSetupRequest,
    ) -> Result<StreamOrigin, WorkflowError> {
        let key = journal::key(request)?;
        let (record, bytes) = match self.store.read_record(&key)? {
            Some(bytes) => {
                let record = journal::decode(&bytes)?;
                journal::validate(&record, request)?;
                (record, bytes)
            }
            None => {
                let record = journal::prepare(request.clone())?;
                let bytes = journal::encode(&record)?;
                match self.store.compare_exchange_record(&key, None, &bytes) {
                    Ok(()) => (record, bytes),
                    Err(StoreError::RecordConflict(_)) => {
                        let raced = self.store.read_record(&key)?.ok_or_else(|| {
                            failure("registered setup journal disappeared during prepare")
                        })?;
                        let raced_record = journal::decode(&raced)?;
                        journal::validate(&raced_record, request)?;
                        (raced_record, raced)
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        };
        if let Some(receipt) = record.receipt {
            if self.registered_setup_children(request)? != receipt {
                return Err(failure("completed registered setup child effect mismatch"));
            }
            return Ok(receipt);
        }
        let receipt = self.registered_setup_children(request)?;
        let replacement = journal::encode(&journal::complete(record, receipt)?)?;
        match self
            .store
            .compare_exchange_record(&key, Some(&bytes), &replacement)
        {
            Ok(()) => Ok(receipt),
            Err(StoreError::RecordConflict(_)) => {
                let current = self.store.read_record(&key)?.ok_or_else(|| {
                    failure("registered setup journal disappeared during completion")
                })?;
                let current = journal::decode(&current)?;
                journal::validate(&current, request)?;
                if current.receipt != Some(receipt)
                    || self.registered_setup_children(request)? != receipt
                {
                    return Err(failure("racing registered setup completion mismatch"));
                }
                Ok(receipt)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn registered_setup_children(
        &mut self,
        request: &RegisteredSetupRequest,
    ) -> Result<StreamOrigin, WorkflowError> {
        let baseline =
            PenelopeInferenceMaterializer::new(&mut self.store).run_registered(&request.rebuild)?;
        PenelopeInferenceStream::new(&mut self.store).begin(
            &request.rebuild.database,
            &StreamBeginRequest {
                baseline,
                claim_budget: request.rebuild.limits.claims,
                byte_budget: request.rebuild.output_bytes,
                premise_budget: request.rebuild.premise_budget,
                page_budget: request.page_budget,
            },
        )
    }
}

#[cfg(test)]
#[path = "registered/tests.rs"]
mod tests;
