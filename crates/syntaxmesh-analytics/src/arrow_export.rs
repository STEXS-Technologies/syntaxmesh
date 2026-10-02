//! Bounded typed Arrow feeds derived from canonical SyntaxMesh history.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use arrow_array::builder::{BinaryBuilder, FixedSizeBinaryBuilder, StringBuilder};
use arrow_array::{
    ArrayRef, BooleanArray, RecordBatch, StringArray, UInt8Array, UInt16Array, UInt32Array,
    UInt64Array,
};
use arrow_schema::{ArrowError, DataType, Field, Schema, SchemaRef};
use syntaxmesh_core::{
    AcceptanceTime, EvidenceClass, ExportKind, FactPayload, FactRef, GenerationId, ImportKind,
    ModuleResolutionDiagnosticStatus, NodeKind, RelationKind, RepositoryId, WorktreeId,
};
use syntaxmesh_store::{
    FactHistoryCursor, FactHistoryEntry, FactHistoryPage, FactVersionChangePage, GraphStore,
    StoreError,
};

/// Version of the stable `fact_history_attributes` Arrow schema.
pub const FACT_HISTORY_ARROW_SCHEMA_VERSION: u16 = 2;

/// Maximum facts read into one Arrow page, bounding memory independently of
/// total retained history size.
pub const MAX_FACT_HISTORY_PAGE_SIZE: usize = 256;

const VALUE_TEXT: u8 = 1;
const VALUE_BINARY: u8 = 2;
const VALUE_UINT64: u8 = 3;
const VALUE_BOOLEAN: u8 = 4;

/// Errors raised while reading canonical history or building an Arrow page.
#[derive(Debug)]
pub enum AnalyticsError {
    Store(StoreError),
    Arrow(ArrowError),
    InvalidPageSize(usize),
}

impl Display for AnalyticsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for AnalyticsError {}

impl From<StoreError> for AnalyticsError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ArrowError> for AnalyticsError {
    fn from(error: ArrowError) -> Self {
        Self::Arrow(error)
    }
}

/// One Arrow batch and the cursor that can be checkpointed after its sink has
/// durably accepted the batch.
#[derive(Debug, Clone)]
pub struct FactHistoryArrowBatch {
    pub batch: RecordBatch,
    pub resume_after: Option<FactHistoryCursor>,
}

/// Lazy reader over the globally ordered fact-version history at one pinned
/// graph generation. Each iteration reads and converts at most one page.
pub struct FactHistoryBatchReader<'store, S: GraphStore + ?Sized> {
    store: &'store S,
    as_of_generation: GenerationId,
    as_of_generation_sequence: u64,
    repository: RepositoryId,
    worktree: WorktreeId,
    page_size: usize,
    cursor: Option<FactHistoryCursor>,
    done: bool,
}

impl<'store, S: GraphStore + ?Sized> FactHistoryBatchReader<'store, S> {
    /// Create a bounded reader for one retained generation.
    ///
    /// # Errors
    /// Returns an error for an invalid page size or unavailable generation.
    pub fn new(
        store: &'store S,
        as_of_generation: GenerationId,
        page_size: usize,
        resume_after: Option<FactHistoryCursor>,
    ) -> Result<Self, AnalyticsError> {
        if page_size == 0 || page_size > MAX_FACT_HISTORY_PAGE_SIZE {
            return Err(AnalyticsError::InvalidPageSize(page_size));
        }
        if resume_after.is_some_and(|cursor| cursor.as_of_generation != as_of_generation) {
            return Err(AnalyticsError::Store(StoreError::InvalidDelta(
                "analytics cursor is bound to a different generation".to_owned(),
            )));
        }
        let manifest = store.manifest(as_of_generation)?;
        let as_of_generation_sequence = store.generation_sequence(as_of_generation)?;
        Ok(Self {
            store,
            as_of_generation,
            as_of_generation_sequence,
            repository: manifest.repository,
            worktree: manifest.worktree,
            page_size,
            cursor: resume_after,
            done: false,
        })
    }

    /// Snapshot generation bound to every batch from this reader.
    #[must_use]
    pub const fn as_of_generation(&self) -> GenerationId {
        self.as_of_generation
    }
}

impl<S: GraphStore + ?Sized> Iterator for FactHistoryBatchReader<'_, S> {
    type Item = Result<FactHistoryArrowBatch, AnalyticsError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let page =
            match self
                .store
                .fact_history_page(self.as_of_generation, self.cursor, self.page_size)
            {
                Ok(page) => page,
                Err(error) => {
                    self.done = true;
                    return Some(Err(AnalyticsError::Store(error)));
                }
            };
        if page.items.is_empty() {
            self.done = true;
            return None;
        }
        let resume_after = page.items.last().map(|entry| FactHistoryCursor {
            as_of_generation: self.as_of_generation,
            after_fact: entry.fact,
            after_valid_from: entry.version.valid_from,
        });
        self.cursor = page.next_cursor;
        self.done = self.cursor.is_none();
        let batch = fact_history_page_to_batch(
            &page,
            self.as_of_generation,
            self.as_of_generation_sequence,
            self.repository,
            self.worktree,
        )
        .map(|batch| FactHistoryArrowBatch {
            batch,
            resume_after,
        });
        if batch.is_err() {
            self.done = true;
        }
        Some(batch.map_err(AnalyticsError::Arrow))
    }
}

/// Convert one bounded store page to the versioned, typed attribute schema.
///
/// # Errors
/// Returns an Arrow error if a value cannot be represented by the schema.
pub fn fact_history_page_to_batch(
    page: &FactHistoryPage,
    as_of_generation: GenerationId,
    as_of_generation_sequence: u64,
    repository: RepositoryId,
    worktree: WorktreeId,
) -> Result<RecordBatch, ArrowError> {
    fact_history_entries_to_batch(
        &page.items,
        as_of_generation,
        as_of_generation_sequence,
        repository,
        worktree,
    )
}

/// Convert one bounded generation-change page to the stable typed attribute
/// schema used by full-history feeds and Parquet rebuilds.
///
/// # Errors
/// Returns an Arrow error if a value cannot be represented by the schema.
pub fn fact_version_changes_page_to_batch(
    page: &FactVersionChangePage,
    generation: GenerationId,
    generation_sequence: u64,
    repository: RepositoryId,
    worktree: WorktreeId,
) -> Result<RecordBatch, ArrowError> {
    fact_history_entries_to_batch(
        &page.items,
        generation,
        generation_sequence,
        repository,
        worktree,
    )
}

fn fact_history_entries_to_batch(
    entries: &[FactHistoryEntry],
    as_of_generation: GenerationId,
    as_of_generation_sequence: u64,
    repository: RepositoryId,
    worktree: WorktreeId,
) -> Result<RecordBatch, ArrowError> {
    let mut rows = Vec::new();
    for entry in entries {
        let (fact_kind, fact_id) = fact_identity(entry.fact);
        let mut attributes = Vec::new();
        append_payload_attributes(&mut attributes, &entry.version.payload);
        for attribute in attributes {
            rows.push(ArrowRow {
                schema_version: FACT_HISTORY_ARROW_SCHEMA_VERSION,
                repository_id: repository.0.0,
                worktree_id: worktree.0.0,
                as_of_generation: as_of_generation.0.0,
                fact_kind,
                fact_id,
                valid_from: entry.version.valid_from.0.0,
                valid_until: entry.version.valid_until.map(|id| id.0.0),
                as_of_generation_sequence,
                valid_from_sequence: entry.valid_from_sequence,
                valid_until_sequence: entry.valid_until_sequence,
                observed_at: entry.version.observed_at.map(|time| time.0),
                accepted_at: entry.version.accepted_at,
                attribute,
            });
        }
    }
    build_batch(rows)
}

#[derive(Debug)]
enum AttributeValue {
    Text(String),
    Binary(Vec<u8>),
    Unsigned(u64),
    Boolean(bool),
}

#[derive(Debug)]
struct Attribute {
    name: String,
    ordinal: u32,
    value: AttributeValue,
}

#[derive(Debug)]
struct ArrowRow {
    schema_version: u16,
    repository_id: [u8; 32],
    worktree_id: [u8; 32],
    as_of_generation: [u8; 32],
    fact_kind: &'static str,
    fact_id: [u8; 32],
    valid_from: [u8; 32],
    valid_until: Option<[u8; 32]>,
    as_of_generation_sequence: u64,
    valid_from_sequence: u64,
    valid_until_sequence: Option<u64>,
    observed_at: Option<u64>,
    accepted_at: Option<AcceptanceTime>,
    attribute: Attribute,
}

fn build_batch(rows: Vec<ArrowRow>) -> Result<RecordBatch, ArrowError> {
    let schema = fact_history_arrow_schema();
    let mut schema_versions = Vec::with_capacity(rows.len());
    let mut repository_ids = FixedSizeBinaryBuilder::with_capacity(rows.len(), 32);
    let mut worktree_ids = FixedSizeBinaryBuilder::with_capacity(rows.len(), 32);
    let mut as_of_generations = FixedSizeBinaryBuilder::with_capacity(rows.len(), 32);
    let mut fact_kinds = Vec::with_capacity(rows.len());
    let mut fact_ids = FixedSizeBinaryBuilder::with_capacity(rows.len(), 32);
    let mut valid_froms = FixedSizeBinaryBuilder::with_capacity(rows.len(), 32);
    let mut valid_untils = FixedSizeBinaryBuilder::with_capacity(rows.len(), 32);
    let mut as_of_generation_sequences = Vec::with_capacity(rows.len());
    let mut valid_from_sequences = Vec::with_capacity(rows.len());
    let mut valid_until_sequences = Vec::with_capacity(rows.len());
    let mut observed_at = Vec::with_capacity(rows.len());
    let mut accepted_at = Vec::with_capacity(rows.len());
    let mut attributes = Vec::with_capacity(rows.len());
    let mut ordinals = Vec::with_capacity(rows.len());
    let mut value_kinds = Vec::with_capacity(rows.len());
    let mut value_text = StringBuilder::new();
    let mut value_binary = BinaryBuilder::new();
    let mut value_uint64 = Vec::with_capacity(rows.len());
    let mut value_boolean = Vec::with_capacity(rows.len());

    for row in rows {
        schema_versions.push(row.schema_version);
        repository_ids.append_value(row.repository_id.as_slice())?;
        worktree_ids.append_value(row.worktree_id.as_slice())?;
        as_of_generations.append_value(row.as_of_generation.as_slice())?;
        fact_kinds.push(row.fact_kind);
        fact_ids.append_value(row.fact_id.as_slice())?;
        valid_froms.append_value(row.valid_from.as_slice())?;
        append_fixed_binary_optional(&mut valid_untils, row.valid_until)?;
        as_of_generation_sequences.push(row.as_of_generation_sequence);
        valid_from_sequences.push(row.valid_from_sequence);
        valid_until_sequences.push(row.valid_until_sequence);
        observed_at.push(row.observed_at);
        accepted_at.push(row.accepted_at.map(|time| time.0));
        attributes.push(row.attribute.name);
        ordinals.push(row.attribute.ordinal);
        match row.attribute.value {
            AttributeValue::Text(value) => {
                value_kinds.push(VALUE_TEXT);
                value_text.append_value(value);
                value_binary.append_null();
                value_uint64.push(None);
                value_boolean.push(None);
            }
            AttributeValue::Binary(value) => {
                value_kinds.push(VALUE_BINARY);
                value_text.append_null();
                value_binary.append_value(value);
                value_uint64.push(None);
                value_boolean.push(None);
            }
            AttributeValue::Unsigned(value) => {
                value_kinds.push(VALUE_UINT64);
                value_text.append_null();
                value_binary.append_null();
                value_uint64.push(Some(value));
                value_boolean.push(None);
            }
            AttributeValue::Boolean(value) => {
                value_kinds.push(VALUE_BOOLEAN);
                value_text.append_null();
                value_binary.append_null();
                value_uint64.push(None);
                value_boolean.push(Some(value));
            }
        }
    }

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(UInt16Array::from(schema_versions)) as ArrayRef,
            Arc::new(repository_ids.finish()),
            Arc::new(worktree_ids.finish()),
            Arc::new(as_of_generations.finish()),
            Arc::new(StringArray::from(fact_kinds)) as ArrayRef,
            Arc::new(fact_ids.finish()),
            Arc::new(valid_froms.finish()),
            Arc::new(valid_untils.finish()),
            Arc::new(StringArray::from(attributes)) as ArrayRef,
            Arc::new(UInt32Array::from(ordinals)) as ArrayRef,
            Arc::new(UInt8Array::from(value_kinds)) as ArrayRef,
            Arc::new(value_text.finish()),
            Arc::new(value_binary.finish()),
            Arc::new(UInt64Array::from(observed_at)) as ArrayRef,
            Arc::new(UInt64Array::from(accepted_at)) as ArrayRef,
            Arc::new(UInt64Array::from(value_uint64)) as ArrayRef,
            Arc::new(BooleanArray::from(value_boolean)) as ArrayRef,
            Arc::new(UInt64Array::from(as_of_generation_sequences)) as ArrayRef,
            Arc::new(UInt64Array::from(valid_from_sequences)) as ArrayRef,
            Arc::new(UInt64Array::from(valid_until_sequences)) as ArrayRef,
        ],
    )
}

/// Stable schema shared by every page from the fact-history feed.
#[must_use]
pub fn fact_history_arrow_schema() -> SchemaRef {
    Arc::new(Schema::new_with_metadata(
        vec![
            Field::new("schema_version", DataType::UInt16, false),
            Field::new("repository_id", DataType::FixedSizeBinary(32), false),
            Field::new("worktree_id", DataType::FixedSizeBinary(32), false),
            Field::new("as_of_generation", DataType::FixedSizeBinary(32), false),
            Field::new("fact_kind", DataType::Utf8, false),
            Field::new("fact_id", DataType::FixedSizeBinary(32), false),
            Field::new(
                "valid_from_generation",
                DataType::FixedSizeBinary(32),
                false,
            ),
            Field::new(
                "valid_until_generation",
                DataType::FixedSizeBinary(32),
                true,
            ),
            Field::new("attribute", DataType::Utf8, false),
            Field::new("ordinal", DataType::UInt32, false),
            Field::new("value_kind", DataType::UInt8, false),
            Field::new("value_text", DataType::Utf8, true),
            Field::new("value_binary", DataType::Binary, true),
            Field::new("observed_at_unix_nanos", DataType::UInt64, true),
            Field::new("accepted_at_unix_nanos", DataType::UInt64, true),
            Field::new("value_uint64", DataType::UInt64, true),
            Field::new("value_boolean", DataType::Boolean, true),
            Field::new("as_of_generation_sequence", DataType::UInt64, false),
            Field::new("valid_from_generation_sequence", DataType::UInt64, false),
            Field::new("valid_until_generation_sequence", DataType::UInt64, true),
        ],
        std::collections::HashMap::from([
            (
                "syntaxmesh.dataset".to_owned(),
                "fact_history_attributes".to_owned(),
            ),
            (
                "syntaxmesh.schema_version".to_owned(),
                FACT_HISTORY_ARROW_SCHEMA_VERSION.to_string(),
            ),
        ]),
    ))
}

fn append_fixed_binary_optional(
    builder: &mut FixedSizeBinaryBuilder,
    value: Option<[u8; 32]>,
) -> Result<(), ArrowError> {
    if let Some(bytes) = value {
        builder.append_value(bytes)?;
    } else {
        builder.append_null();
    }
    Ok(())
}

const fn fact_identity(fact: FactRef) -> (&'static str, [u8; 32]) {
    match fact {
        FactRef::File(id) => ("file", id.0.0),
        FactRef::Provenance(id) => ("provenance", id.0.0),
        FactRef::Node(id) => ("node", id.0.0),
        FactRef::Edge(id) => ("edge", id.0.0),
    }
}

fn append_payload_attributes(attributes: &mut Vec<Attribute>, payload: &FactPayload) {
    match payload {
        FactPayload::File(file) => {
            text(attributes, "file.path", &file.normalized_path);
            binary(attributes, "file.content_hash", file.content_hash);
            unsigned(attributes, "file.size_bytes", file.size_bytes);
        }
        FactPayload::Provenance(provenance) => {
            text(
                attributes,
                "provenance.producer_namespace",
                &provenance.producer_namespace,
            );
            text(
                attributes,
                "provenance.producer_version",
                &provenance.producer_version,
            );
            text(
                attributes,
                "provenance.evidence_class",
                evidence_class_name(provenance.evidence_class),
            );
            if let Some(source) = &provenance.source {
                source_attributes(attributes, "provenance.source", source);
            }
        }
        FactPayload::Node(node) => {
            text(attributes, "node.name", &node.name);
            node_kind_attributes(attributes, &node.kind);
            if let Some(owner) = node.owner_file {
                binary(attributes, "node.owner_file_id", owner.0.0);
            }
            binary(attributes, "node.provenance_id", node.provenance.0.0);
            if let Some(source) = &node.source {
                source_attributes(attributes, "node.source", source);
            }
            if let Some(extension) = &node.extension_payload {
                text(attributes, "node.extension.namespace", &extension.namespace);
                unsigned(
                    attributes,
                    "node.extension.schema_version",
                    u64::from(extension.schema_version),
                );
                binary_vec(attributes, "node.extension.bytes", &extension.bytes);
            }
        }
        FactPayload::Edge(edge) => {
            binary(attributes, "edge.source_id", edge.source.0.0);
            binary(attributes, "edge.target_id", edge.target.0.0);
            relation_attributes(attributes, "edge.relation", &edge.relation);
            binary(attributes, "edge.provenance_id", edge.provenance.0.0);
            if let Some(extension) = &edge.extension_payload {
                text(attributes, "edge.extension.namespace", &extension.namespace);
                unsigned(
                    attributes,
                    "edge.extension.schema_version",
                    u64::from(extension.schema_version),
                );
                binary_vec(attributes, "edge.extension.bytes", &extension.bytes);
            }
        }
    }
}

fn source_attributes(
    attributes: &mut Vec<Attribute>,
    prefix: &str,
    source: &syntaxmesh_core::SourceLocation,
) {
    binary(attributes, &format!("{prefix}.file_id"), source.file_id.0.0);
    binary(
        attributes,
        &format!("{prefix}.content_hash"),
        source.content_hash,
    );
    unsigned(
        attributes,
        &format!("{prefix}.start_byte"),
        source.span.start_byte,
    );
    unsigned(
        attributes,
        &format!("{prefix}.end_byte"),
        source.span.end_byte,
    );
}

fn node_kind_attributes(attributes: &mut Vec<Attribute>, kind: &NodeKind) {
    let name = match kind {
        NodeKind::Repository => "repository",
        NodeKind::File => "file",
        NodeKind::Module => "module",
        NodeKind::Function => "function",
        NodeKind::Struct => "struct",
        NodeKind::Enum => "enum",
        NodeKind::Trait => "trait",
        NodeKind::Test => "test",
        NodeKind::External { namespace, kind } => {
            text(attributes, "node.kind.namespace", namespace);
            text(attributes, "node.kind.external_name", kind);
            "external"
        }
        NodeKind::Reference { relation: value } => {
            relation_attributes(attributes, "node.kind.relation", value);
            "reference"
        }
        NodeKind::UnresolvedReference { relation: value } => {
            relation_attributes(attributes, "node.kind.relation", value);
            "unresolved_reference"
        }
        NodeKind::AmbiguousReference { relation: value } => {
            relation_attributes(attributes, "node.kind.relation", value);
            "ambiguous_reference"
        }
        NodeKind::RuntimeObservation => "runtime_observation",
        NodeKind::Class => "class",
        NodeKind::Import {
            specifier,
            kind,
            imported_name,
            local_name,
            type_only,
        } => {
            text(attributes, "node.kind.import.specifier", specifier);
            text(
                attributes,
                "node.kind.import.syntax",
                import_kind_name(*kind),
            );
            optional_text(
                attributes,
                "node.kind.import.imported_name",
                imported_name.as_deref(),
            );
            optional_text(
                attributes,
                "node.kind.import.local_name",
                local_name.as_deref(),
            );
            boolean(attributes, "node.kind.import.type_only", *type_only);
            "import"
        }
        NodeKind::Export {
            source_specifier,
            kind,
            exported_name,
            local_name,
            type_only,
        } => {
            optional_text(
                attributes,
                "node.kind.export.source_specifier",
                source_specifier.as_deref(),
            );
            text(
                attributes,
                "node.kind.export.syntax",
                export_kind_name(*kind),
            );
            optional_text(
                attributes,
                "node.kind.export.exported_name",
                exported_name.as_deref(),
            );
            optional_text(
                attributes,
                "node.kind.export.local_name",
                local_name.as_deref(),
            );
            boolean(attributes, "node.kind.export.type_only", *type_only);
            "export"
        }
        NodeKind::ModuleResolutionDiagnostic {
            occurrence,
            status,
            candidate_paths,
        } => {
            binary(
                attributes,
                "node.kind.diagnostic.occurrence_id",
                occurrence.0.0,
            );
            text(
                attributes,
                "node.kind.diagnostic.status",
                diagnostic_status_name(*status),
            );
            for (index, path) in candidate_paths.iter().enumerate() {
                text_at(
                    attributes,
                    "node.kind.diagnostic.candidate_path",
                    index,
                    path,
                );
            }
            "module_resolution_diagnostic"
        }
        NodeKind::Script => "script",
        NodeKind::Document => "document",
        NodeKind::Section => "section",
        NodeKind::DocumentChunk => "document_chunk",
    };
    text(attributes, "node.kind", name);
}

fn relation_attributes(attributes: &mut Vec<Attribute>, prefix: &str, relation: &RelationKind) {
    let name = match relation {
        RelationKind::Defines => "defines",
        RelationKind::Imports => "imports",
        RelationKind::References => "references",
        RelationKind::Calls => "calls",
        RelationKind::Implements => "implements",
        RelationKind::External {
            namespace,
            relation,
        } => {
            text(attributes, &format!("{prefix}.namespace"), namespace);
            text(attributes, &format!("{prefix}.external_name"), relation);
            "external"
        }
        RelationKind::ResolvesTo => "resolves_to",
        RelationKind::Exports => "exports",
        RelationKind::HasResolutionDiagnostic => "has_resolution_diagnostic",
        RelationKind::Contains => "contains",
    };
    text(attributes, &format!("{prefix}.kind"), name);
}

const fn evidence_class_name(value: EvidenceClass) -> &'static str {
    match value {
        EvidenceClass::SourceFact => "source_fact",
        EvidenceClass::StaticallyResolved => "statically_resolved",
        EvidenceClass::Heuristic => "heuristic",
        EvidenceClass::SemanticInference => "semantic_inference",
        EvidenceClass::UserAsserted => "user_asserted",
        EvidenceClass::RuntimeObserved => "runtime_observed",
        EvidenceClass::CommittedTransition => "committed_transition",
        EvidenceClass::ResolutionDiagnostic => "resolution_diagnostic",
    }
}

const fn import_kind_name(value: ImportKind) -> &'static str {
    match value {
        ImportKind::SideEffect => "side_effect",
        ImportKind::Default => "default",
        ImportKind::Named => "named",
        ImportKind::Namespace => "namespace",
        ImportKind::NamespaceMember => "namespace_member",
        ImportKind::Dynamic => "dynamic",
        ImportKind::CommonJs => "common_js",
        ImportKind::PythonModule => "python_module",
        ImportKind::PythonFrom => "python_from",
        ImportKind::PythonStar => "python_star",
        ImportKind::RustUse => "rust_use",
        ImportKind::RustGlob => "rust_glob",
    }
}

const fn export_kind_name(value: ExportKind) -> &'static str {
    match value {
        ExportKind::Local => "local",
        ExportKind::Default => "default",
        ExportKind::NamedReExport => "named_re_export",
        ExportKind::NamespaceReExport => "namespace_re_export",
        ExportKind::StarReExport => "star_re_export",
    }
}

const fn diagnostic_status_name(value: ModuleResolutionDiagnosticStatus) -> &'static str {
    match value {
        ModuleResolutionDiagnosticStatus::Unresolved => "unresolved",
        ModuleResolutionDiagnosticStatus::Ambiguous => "ambiguous",
        ModuleResolutionDiagnosticStatus::Invalid => "invalid",
        ModuleResolutionDiagnosticStatus::TargetNotIndexed => "target_not_indexed",
    }
}

fn text(attributes: &mut Vec<Attribute>, name: &str, value: &str) {
    text_at(attributes, name, 0, value);
}

fn text_at(attributes: &mut Vec<Attribute>, name: &str, ordinal: usize, value: &str) {
    attributes.push(Attribute {
        name: name.to_owned(),
        ordinal: u32::try_from(ordinal).unwrap_or(u32::MAX),
        value: AttributeValue::Text(value.to_owned()),
    });
}

fn optional_text(attributes: &mut Vec<Attribute>, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        text(attributes, name, value);
    }
}

fn binary(attributes: &mut Vec<Attribute>, name: &str, value: [u8; 32]) {
    binary_vec(attributes, name, &value);
}

fn binary_vec(attributes: &mut Vec<Attribute>, name: &str, value: &[u8]) {
    attributes.push(Attribute {
        name: name.to_owned(),
        ordinal: 0,
        value: AttributeValue::Binary(value.to_vec()),
    });
}

fn unsigned(attributes: &mut Vec<Attribute>, name: &str, value: u64) {
    attributes.push(Attribute {
        name: name.to_owned(),
        ordinal: 0,
        value: AttributeValue::Unsigned(value),
    });
}

fn boolean(attributes: &mut Vec<Attribute>, name: &str, value: bool) {
    attributes.push(Attribute {
        name: name.to_owned(),
        ordinal: 0,
        value: AttributeValue::Boolean(value),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::{Array, FixedSizeBinaryArray, StringArray};
    use syntaxmesh_core::{
        Edge, EdgeId, EvidenceClass, ExtensionPayload, FileVersion, GraphDelta, ImportKind,
        IndexRunId, ModuleResolutionDiagnosticStatus, Node, NodeId, Provenance, ProvenanceId,
        RelationKind, RepositoryId, SourceLocation, SourceSpan, WorktreeId,
    };
    use syntaxmesh_store::{GraphStore, InMemoryGraphStore};

    #[test]
    fn reader_emits_bounded_versioned_arrow_pages_with_binary_ids() -> Result<(), AnalyticsError> {
        let mut store = InMemoryGraphStore::new();
        let repository = RepositoryId::derive(&[b"analytics-repository"]);
        let worktree = WorktreeId::derive(&[b"analytics-worktree"]);
        let generation = GenerationId::derive(&[b"analytics-generation"]);
        let file_id = syntaxmesh_core::FileId::derive(&[b"analytics-file"]);
        let content = b"source";
        let hash = *blake3::hash(content).as_bytes();
        let location = SourceLocation {
            file_id,
            content_hash: hash,
            span: SourceSpan {
                start_byte: 0,
                end_byte: 6,
            },
        };
        let provenance = Provenance {
            id: ProvenanceId::derive(&[b"analytics-provenance"]),
            producer_namespace: "syntaxmesh.analytics.test".to_owned(),
            producer_version: "1".to_owned(),
            evidence_class: EvidenceClass::SourceFact,
            source: Some(location.clone()),
        };
        let node = Node {
            id: NodeId::derive(&[b"analytics-node"]),
            kind: NodeKind::Function,
            name: "unit".to_owned(),
            owner_file: Some(file_id),
            source: Some(location.clone()),
            provenance: provenance.id,
            extension_payload: None,
        };
        let import = Node {
            id: NodeId::derive(&[b"analytics-import"]),
            kind: NodeKind::Import {
                specifier: "crate::worker".to_owned(),
                kind: ImportKind::RustUse,
                imported_name: Some("worker".to_owned()),
                local_name: Some("worker_alias".to_owned()),
                type_only: false,
            },
            name: "worker_alias".to_owned(),
            owner_file: Some(file_id),
            source: Some(location.clone()),
            provenance: provenance.id,
            extension_payload: Some(ExtensionPayload {
                namespace: "example.extension".to_owned(),
                schema_version: 1,
                bytes: vec![4, 5, 6],
            }),
        };
        let diagnostic = Node {
            id: NodeId::derive(&[b"analytics-diagnostic"]),
            kind: NodeKind::ModuleResolutionDiagnostic {
                occurrence: import.id,
                status: ModuleResolutionDiagnosticStatus::Ambiguous,
                candidate_paths: vec!["src/worker.rs".to_owned(), "src/worker/mod.rs".to_owned()],
            },
            name: "ambiguous worker import".to_owned(),
            owner_file: Some(file_id),
            source: Some(location),
            provenance: provenance.id,
            extension_payload: None,
        };
        store.apply_delta(GraphDelta {
            repository,
            worktree,
            run_id: IndexRunId::derive(&[b"analytics-run"]),
            expected_base: None,
            next_generation: generation,
            changed_files: vec![FileVersion {
                file_id,
                normalized_path: "src/unit.rs".to_owned(),
                content_hash: hash,
                size_bytes: u64::try_from(content.len()).unwrap_or(u64::MAX),
            }],
            removed_files: Vec::new(),
            upsert_provenance: vec![provenance],
            upsert_nodes: vec![node.clone(), import, diagnostic],
            upsert_edges: vec![Edge {
                id: EdgeId::derive(&[b"analytics-edge"]),
                source: node.id,
                target: node.id,
                relation: RelationKind::Calls,
                provenance: node.provenance,
                extension_payload: None,
            }],
            remove_nodes: Vec::new(),
            remove_edges: Vec::new(),
        })?;

        let reader = FactHistoryBatchReader::new(&store, generation, 2, None)?;
        let mut batches = Vec::new();
        for batch in reader {
            batches.push(batch?);
        }
        let total_rows = batches
            .iter()
            .map(|batch| batch.batch.num_rows())
            .sum::<usize>();
        if batches.len() != 3 || total_rows < 20 {
            return Err(AnalyticsError::Arrow(ArrowError::ComputeError(format!(
                "expected three bounded Arrow pages and typed attributes, got {} pages and {total_rows} rows",
                batches.len()
            ))));
        }
        let first_batch = &batches
            .first()
            .ok_or_else(|| {
                AnalyticsError::Arrow(ArrowError::ComputeError(
                    "first analytics batch is missing".to_owned(),
                ))
            })?
            .batch;
        if first_batch
            .schema()
            .metadata()
            .get("syntaxmesh.schema_version")
            != Some(&FACT_HISTORY_ARROW_SCHEMA_VERSION.to_string())
            || first_batch.schema().field(1).data_type() != &DataType::FixedSizeBinary(32)
        {
            return Err(AnalyticsError::Arrow(ArrowError::SchemaError(
                "Arrow feed is missing its version metadata or binary IDs".to_owned(),
            )));
        }
        let ids = first_batch
            .column(5)
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .ok_or_else(|| ArrowError::CastError("fact_id is not binary".to_owned()))?;
        if ids.value_length() != 32 {
            return Err(AnalyticsError::Arrow(ArrowError::ComputeError(
                "stable IDs must be 32-byte binary values".to_owned(),
            )));
        }
        let fact_kinds = first_batch
            .column(4)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| ArrowError::CastError("fact_kind is not utf8".to_owned()))?;
        if fact_kinds.is_empty() || fact_kinds.value(0).is_empty() {
            return Err(AnalyticsError::Arrow(ArrowError::ComputeError(
                "fact family label is missing".to_owned(),
            )));
        }
        let mut candidate_paths = Vec::new();
        for batch in &batches {
            let names = batch
                .batch
                .column(8)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| ArrowError::CastError("attribute is not utf8".to_owned()))?;
            let ordinals = batch
                .batch
                .column(9)
                .as_any()
                .downcast_ref::<UInt32Array>()
                .ok_or_else(|| ArrowError::CastError("ordinal is not uint32".to_owned()))?;
            let values = batch
                .batch
                .column(11)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or_else(|| ArrowError::CastError("text value is not utf8".to_owned()))?;
            for index in 0..batch.batch.num_rows() {
                if names.value(index) == "node.kind.diagnostic.candidate_path" {
                    candidate_paths.push((ordinals.value(index), values.value(index).to_owned()));
                }
            }
        }
        if candidate_paths
            != vec![
                (0, "src/worker.rs".to_owned()),
                (1, "src/worker/mod.rs".to_owned()),
            ]
        {
            return Err(AnalyticsError::Arrow(ArrowError::ComputeError(
                "repeated enum fields did not preserve their deterministic ordinals".to_owned(),
            )));
        }
        let resume_after = batches
            .first()
            .and_then(|batch| batch.resume_after)
            .ok_or_else(|| {
                AnalyticsError::Arrow(ArrowError::ComputeError(
                    "first Arrow page has no restart cursor".to_owned(),
                ))
            })?;
        let resumed_reader =
            FactHistoryBatchReader::new(&store, generation, 2, Some(resume_after))?;
        let resumed_rows = resumed_reader
            .map(|batch| batch.map(|page| page.batch.num_rows()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum::<usize>();
        let first_rows = batches
            .first()
            .map(|batch| batch.batch.num_rows())
            .unwrap_or_default();
        if resumed_rows.saturating_add(first_rows) != total_rows {
            return Err(AnalyticsError::Arrow(ArrowError::ComputeError(
                "restarting after a committed Arrow page duplicated or omitted rows".to_owned(),
            )));
        }
        Ok(())
    }
}
