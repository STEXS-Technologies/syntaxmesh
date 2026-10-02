use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;

use syntaxmesh_api_model::{GRAPH_EXPORT_SCHEMA_VERSION, GraphRecord, TemporalQueryMode};
use syntaxmesh_core::{
    AcceptanceTime, AcceptedGenerationCursor, ChangeSetId, EventsForChangeSetCursor, GenerationId,
    ObservedFactCursor,
};
use syntaxmesh_engine::SyntaxMeshEngine;
use syntaxmesh_lang_rust::RustExtractor;
use syntaxmesh_query::ConsequenceTraceRequest;
use syntaxmesh_query::QueryError;
use syntaxmesh_store::{DurableRecordStore, FileGraphStore, GraphStore};
use syntaxmesh_store_turso::TursoGraphStore;

mod attachment;
mod cycles;
mod error;
mod extensions;
mod generations;
#[path = "git_context.rs"]
mod git_context;
mod graph;
mod history;
mod identifiers;
mod indexing;
mod ownership;
mod processing;
#[path = "project_config.rs"]
mod project_config;
mod rejections;
mod semantic;
mod status;
mod timelines;
mod watching;

use error::CliError;
use generations::{
    changes, changes_turso, graph_at, graph_at_known_by, graph_at_known_by_turso, graph_at_turso,
};
use graph::{
    HistoricalNeighborhoodLimits, consequence_neighborhood_file, consequence_neighborhood_turso,
    consequence_trace_file, consequence_trace_turso, historical_neighborhood,
    historical_neighborhood_turso, historical_neighbors, historical_neighbors_turso, impact,
    impact_turso, neighbors, neighbors_turso, node, node_at, node_at_turso, node_turso,
    parse_edge_direction, path, path_turso, subgraph, subgraph_turso,
};
use history::{
    fact_history, fact_history_turso, fact_lineage, fact_lineage_turso, history, history_turso,
};
use identifiers::{
    parse_acceptance_time, parse_change_set_id, parse_consequence_seed, parse_edge_id,
    parse_evidence_classes, parse_fact_ref, parse_generation_id, parse_index_run_id, parse_limit,
    parse_node_id, parse_observation_time,
};
use indexing::{index, index_arguments, index_options, index_turso, init_project};
use rejections::{workflow_rejections, workflow_rejections_turso};
use status::{integrity_snapshot, integrity_turso, status, status_turso};
use timelines::{
    accepted_between, accepted_between_turso, change_correlations, change_correlations_turso,
    observed_between, observed_between_turso,
};

const CLI_HELP: &str = "SyntaxMesh local Rust/Python/TypeScript/JavaScript/Bash/documentation graph engine\n\n\
Usage:\n\
  syntaxmesh cycles[-turso] <store> <current|generation-id> <calls|imports|all>\n\
  syntaxmesh watch[-turso] <root> <store-outside-root> [index options] [--poll-only] [--reconcile-ms <positive-ms>] [--watch-duration-ms <positive-ms>]\n\
  syntaxmesh ingest-extension[-turso] <store> <grant.json> <frame> <run-id> <generation-id> [--verify]\n\
  AI harness transport: add --semantic [model] --semantic-command '<JSON argv array>|@path' to index/index-turso\n\
  syntaxmesh init [project-root] [--verified-history] [--node-module-resolution] [--python-module-resolution]\n\
  syntaxmesh index [root] [snapshot] [--verify] [--record-syntax-failures] [--semantic [model] [--semantic-endpoint <url>] [--semantic-api-key-env <name>] [--semantic-model-revision <revision>] [--allow-network]]\n\
  syntaxmesh index-turso <root> <database> [--verify] [--record-syntax-failures] [--semantic [model] [--semantic-endpoint <url>] [--semantic-api-key-env <name>] [--semantic-model-revision <revision>] [--allow-network]]\n\
  syntaxmesh search <snapshot> <text>\n\
  syntaxmesh search-turso <database> <text>\n\
  syntaxmesh node|history <snapshot> <node-id>\n\
  syntaxmesh node-turso|history-turso <database> <node-id>\n\
  syntaxmesh node-at <snapshot> <generation> <node-id>\n\
  syntaxmesh node-at-turso <database> <generation> <node-id>\n\
  syntaxmesh resolution-diagnostics <snapshot>\n\
  syntaxmesh resolution-diagnostics-turso <database>\n\
  syntaxmesh fact-history <snapshot> <file|provenance|node|edge> <id>\n\
  syntaxmesh fact-history-turso <database> <file|provenance|node|edge> <id>\n\
  syntaxmesh fact-lineage <snapshot> <file|provenance|node|edge> <id>\n\
  syntaxmesh fact-lineage-turso <database> <file|provenance|node|edge> <id>\n\
  syntaxmesh change-correlations[-turso] <store> <source-generation> <fact-kind> <fact-id> <limit> [after-generation]\n\
  syntaxmesh graph-at <snapshot> <generation>\n\
  syntaxmesh graph-at-turso <database> <generation>\n\
  syntaxmesh graph-at-known-by <snapshot> <generation> <accepted-by-ns>\n\
  syntaxmesh graph-at-known-by-turso <database> <generation> <accepted-by-ns>\n\
  syntaxmesh changes <snapshot> <from-generation> <to-generation>\n\
  syntaxmesh changes-turso <database> <from-generation> <to-generation>\n\
  syntaxmesh change-set-at[-turso] <store> <change-set-id> <generation>\n\
  syntaxmesh change-set-events[-turso] <store> <change-set-id> <generation> <limit> [after-event-generation]\n\
  syntaxmesh accepted-between[-turso] <store> <from-ns> <until-ns> <limit> [cursor]\n\
  syntaxmesh observed-between[-turso] <store> <from-ns> <until-ns> <limit> [cursor]\n\
  syntaxmesh neighbors[-turso] <store> <node-id>\n\
  syntaxmesh neighbors-at[-turso] <store> <generation> <node-id> <incoming|outgoing> <limit> [after-edge-id]\n\
  syntaxmesh neighborhood-at[-turso] <store> <generation> <seed-id[,seed-id...]> <max-hops> <max-nodes> <max-edges> <max-scanned-incidences> <max-result-bytes>\n\
  syntaxmesh path[-turso] <store> <start-id> <target-id> <max-hops>\n\
  syntaxmesh subgraph[-turso] <store> <seed-id> <max-hops> <max-nodes> <max-edges>\n\
  syntaxmesh consequence-neighborhood[-turso] <store> <generation> <event <id>|fact <kind> <id> <valid-from>> <max-hops> <max-endpoints> <max-edges> <max-scanned-incidences>\n\
  syntaxmesh consequence-trace[-turso] <store> <from-generation> <until-generation> <event <id>|fact <kind> <id> <valid-from>> <max-hops> <max-endpoints> <max-edges> <max-scanned-incidences> <max-result-bytes> [--evidence-classes <class[,class...]>]\n\
  syntaxmesh impact[-turso] <store> <text>\n\
  syntaxmesh export[-turso] <store>\n\
  syntaxmesh status <snapshot> [source-root]\n\
  syntaxmesh status-turso <database> [source-root]\n\
  syntaxmesh processing-coverage[-turso] <store> <current|generation-id>\n\
  syntaxmesh workflow-rejections <snapshot> <limit> [after-run-id]\n\
  syntaxmesh workflow-rejections-turso <database> <limit> [after-run-id]\n\
  syntaxmesh git-context <source-root>\n\
  syntaxmesh integrity <snapshot>\n\
  syntaxmesh integrity-turso <database>\n\
  syntaxmesh statechronicle-verify <snapshot>\n\
  syntaxmesh statechronicle-verify-turso <database>\n\
  syntaxmesh turso-migrate <database>\n\
  syntaxmesh turso-migration-status <database>\n\
  syntaxmesh sqlite-migrate <database>\n\
  syntaxmesh sqlite-migration-status <database>\n\n\
Options:\n\
  -h, --help, help  Show this help\n\
  --verify          Enable StateChronicle verification for this indexing run\n\
  --semantic [model] Opt in to AI document claims; command default: gpt-6-luna; HTTP default: qwen3:latest on local Ollama at 127.0.0.1:11434\n\
  --semantic-command  Trusted executable argv as JSON or @path to a JSON file; an exact {model} argument receives the selected model\n\
  --semantic-cross-document  Add bounded joint-document inference in the same semantic index command\n\
  --allow-network   Required for non-loopback HTTPS semantic endpoints\n\
  --semantic-api-key-env  Read an optional provider key from this environment variable\n\
  --semantic-model-revision  Label a model revision; HTTP can discover the local Ollama digest, but command mode cannot verify remote revisions\n\
  --semantic-offline  Reuse cached document semantics without a provider call or harness launch; requires an asserted revision and conflicts with --allow-network\n\
Command mode: authentication and network policy belong to the harness; --semantic-endpoint, --semantic-api-key-env, and --allow-network are rejected\n\
Examples (from this workspace):\n\
  syntaxmesh index --semantic --semantic-command @fixtures/codex-semantic-command.json\n\
  syntaxmesh index --semantic <model-name> --semantic-command @fixtures/codex-semantic-command.json\n\
Default index paths: root is the current directory; snapshot is <root>/.syntaxmesh/index.snapshot\n\
Project config: `syntaxmesh.toml` at <root> supports [history] verified = true and [module_resolution] profile = \"node\"";

fn print_help() -> Result<(), CliError> {
    write_stdout(CLI_HELP)
}

type FileEngine = SyntaxMeshEngine<FileGraphStore, RustExtractor>;
type TursoEngine = SyntaxMeshEngine<TursoGraphStore, RustExtractor>;

fn file_engine(snapshot: &Path) -> Result<(FileEngine, GenerationId), CliError> {
    let store = FileGraphStore::open(snapshot)?;
    let manifest = store
        .latest_generation()
        .ok_or_else(|| CliError::Usage("snapshot has no published generation".to_owned()))?;
    let engine =
        SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
    Ok((engine, manifest.generation))
}

fn turso_engine(
    lease: &syntaxmesh_ownership_host::WriterLease,
) -> Result<(TursoEngine, GenerationId), CliError> {
    let store = TursoGraphStore::open(lease.target())?;
    let manifest = store
        .latest_generation()
        .ok_or_else(|| CliError::Usage("Turso database has no published generation".to_owned()))?;
    let engine =
        SyntaxMeshEngine::new(store, RustExtractor, manifest.repository, manifest.worktree);
    Ok((engine, manifest.generation))
}

fn verify_statechronicle_file(snapshot: &Path) -> Result<(), CliError> {
    let (mut engine, _) = file_engine(snapshot)?;
    report_statechronicle_verification(engine.verify_statechronicle_history()?)
}

fn verify_statechronicle_turso(database: &Path) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (mut engine, _) = turso_engine(&ownership)?;
    report_statechronicle_verification(engine.verify_statechronicle_history()?)
}

fn report_statechronicle_verification(digest: Option<String>) -> Result<(), CliError> {
    digest.map_or_else(
        || write_stdout("statechronicle_history=not_recorded\n"),
        |digest| {
            write_stdout(&format!(
                "statechronicle_history=verified\nhead_digest={digest}\n"
            ))
        },
    )
}

fn search(snapshot: &Path, text: &str) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    search_engine(&engine, generation, text)
}

fn search_turso(database: &Path, text: &str) -> Result<(), CliError> {
    if let Some(endpoint) = syntaxmesh_ownership_host::OwnerEndpoint::discover(database)? {
        return write_search_nodes(attachment::search_nodes(&endpoint, text)?);
    }
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    search_engine(&engine, generation, text)
}

fn search_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
    text: &str,
) -> Result<(), CliError> {
    let query = engine.query(generation);
    write_search_nodes(query.search(text, 100)?)
}

fn write_search_nodes(nodes: Vec<syntaxmesh_core::Node>) -> Result<(), CliError> {
    for node in nodes {
        write_stdout(&format!(
            "{}\t{:?}\t{}",
            node.name,
            node.kind,
            node.id.0.to_hex()
        ))?;
    }
    Ok(())
}

fn resolution_diagnostics(snapshot: &Path) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    resolution_diagnostics_engine(&engine, generation)
}

fn resolution_diagnostics_turso(database: &Path) -> Result<(), CliError> {
    if let Some(owner) = syntaxmesh_ownership_host::OwnerEndpoint::discover(database)? {
        let (repository, generation, nodes) = attachment::resolution_diagnostics(&owner)?;
        return write_resolution_diagnostics(repository, generation, nodes);
    }
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    resolution_diagnostics_engine(&engine, generation)
}

fn resolution_diagnostics_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
) -> Result<(), CliError> {
    let manifest = engine
        .status()?
        .ok_or_else(|| CliError::Usage("store has no published generation".to_owned()))?
        .manifest;
    let nodes = engine.query(generation).module_resolution_diagnostics()?;
    write_resolution_diagnostics(manifest.repository, generation, nodes)
}

fn write_resolution_diagnostics(
    repository: syntaxmesh_core::RepositoryId,
    generation: GenerationId,
    nodes: Vec<syntaxmesh_core::Node>,
) -> Result<(), CliError> {
    let header = GraphRecord::Header {
        schema_version: GRAPH_EXPORT_SCHEMA_VERSION,
        query_mode: TemporalQueryMode::HistoricalConclusion,
        repository,
        generation,
        accepted_by: None,
        accepted_through: None,
    };
    write_stdout(&header.to_json_line().map_err(CliError::Encode)?)?;
    let node_count = u64::try_from(nodes.len())
        .map_err(|error| CliError::Usage(format!("diagnostic count overflow: {error}")))?;
    for node in nodes {
        let record = GraphRecord::Node {
            query_mode: TemporalQueryMode::HistoricalConclusion,
            node,
        };
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    let footer = GraphRecord::Footer {
        query_mode: TemporalQueryMode::HistoricalConclusion,
        nodes: node_count,
        edges: 0,
        provenance_records: 0,
        truncated: false,
        blake3: None,
    };
    write_stdout(&footer.to_json_line().map_err(CliError::Encode)?)?;
    Ok(())
}

fn change_set_at(
    snapshot: &Path,
    change_set: ChangeSetId,
    generation: GenerationId,
) -> Result<(), CliError> {
    let (engine, _) = file_engine(snapshot)?;
    change_set_at_engine(&engine, change_set, generation)
}

fn change_set_at_turso(
    database: &Path,
    change_set: ChangeSetId,
    generation: GenerationId,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, _) = turso_engine(&ownership)?;
    change_set_at_engine(&engine, change_set, generation)
}

fn change_set_at_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    change_set: ChangeSetId,
    generation: GenerationId,
) -> Result<(), CliError> {
    if let Some(record) = engine.query(generation).change_set_at(change_set)? {
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

fn change_set_events(
    snapshot: &Path,
    change_set: ChangeSetId,
    generation: GenerationId,
    after_event_generation: Option<GenerationId>,
    limit: usize,
) -> Result<(), CliError> {
    let (engine, _) = file_engine(snapshot)?;
    change_set_events_engine(
        &engine,
        change_set,
        generation,
        after_event_generation,
        limit,
    )
}

fn change_set_events_turso(
    database: &Path,
    change_set: ChangeSetId,
    generation: GenerationId,
    after_event_generation: Option<GenerationId>,
    limit: usize,
) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, _) = turso_engine(&ownership)?;
    change_set_events_engine(
        &engine,
        change_set,
        generation,
        after_event_generation,
        limit,
    )
}

fn change_set_events_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    change_set: ChangeSetId,
    generation: GenerationId,
    after_event_generation: Option<GenerationId>,
    limit: usize,
) -> Result<(), CliError> {
    let after = after_event_generation.map(|after_event_generation| EventsForChangeSetCursor {
        change_set,
        as_of_generation: generation,
        after_event_generation,
    });
    for record in engine
        .query(generation)
        .events_for_change_set(change_set, after, limit)?
    {
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

fn export(snapshot: &Path) -> Result<(), CliError> {
    let (engine, generation) = file_engine(snapshot)?;
    export_engine(&engine, generation)
}

fn export_turso(database: &Path) -> Result<(), CliError> {
    let ownership = syntaxmesh_ownership_host::WriterLease::acquire(database)?;
    let (engine, generation) = turso_engine(&ownership)?;
    export_engine(&engine, generation)
}

fn export_engine<S: GraphStore + DurableRecordStore>(
    engine: &SyntaxMeshEngine<S, RustExtractor>,
    generation: GenerationId,
) -> Result<(), CliError> {
    let query = engine.query(generation);
    for record in query.export_records()? {
        write_stdout(&record.to_json_line().map_err(CliError::Encode)?)?;
    }
    Ok(())
}

fn git_context(root: &Path) -> Result<(), CliError> {
    let canonical_root = std::fs::canonicalize(root)?;
    let context = git_context::describe(&canonical_root)
        .map_err(|error| CliError::Usage(error.to_string()))?;
    write_stdout(&context)
}

fn sqlite_migrate(database: &Path) -> Result<(), CliError> {
    let _ownership = ownership::IndexOwnership::acquire(database)?;
    syntaxmesh_store_sqlite::SqliteGraphStore::migrate(database)?;
    sqlite_migration_status(database)
}

fn sqlite_migration_status(database: &Path) -> Result<(), CliError> {
    let status = syntaxmesh_store_sqlite::SqliteGraphStore::migration_status(database)?;
    let current = status
        .current_version
        .map_or_else(|| "uninitialized".to_owned(), |version| version.to_string());
    write_stdout(&format!(
        "sqlite_schema_version={current}\nsqlite_schema_target={}\nsqlite_migration_ledger_validated={}",
        status.target_version, status.migration_ledger_validated
    ))?;
    for migration in status.migrations {
        let state = if migration.applied {
            "applied"
        } else {
            "pending"
        };
        write_stdout(&format!(
            "sqlite_migration={state}\t{}\t{}",
            migration.version, migration.name
        ))?;
    }
    Ok(())
}

fn turso_migrate(database: &Path) -> Result<(), CliError> {
    let _ownership = ownership::IndexOwnership::acquire(database)?;
    TursoGraphStore::migrate(database)?;
    turso_migration_status(database)
}

fn turso_migration_status(database: &Path) -> Result<(), CliError> {
    let status = TursoGraphStore::migration_status(database)?;
    let current = status
        .current_version
        .map_or_else(|| "uninitialized".to_owned(), |version| version.to_string());
    write_stdout(&format!(
        "turso_schema_version={current}\nturso_schema_target={}\nturso_migration_ledger_validated={}",
        status.target_version, status.migration_ledger_validated
    ))?;
    for migration in status.migrations {
        let state = if migration.applied {
            "applied"
        } else {
            "pending"
        };
        write_stdout(&format!(
            "turso_migration={state}\t{}\t{}\t{}",
            migration.from, migration.to, migration.name
        ))?;
    }
    Ok(())
}

fn write_stdout(line: &str) -> Result<(), CliError> {
    let mut stdout = std::io::stdout().lock();
    match writeln!(stdout, "{line}") {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(CliError::Io(error)),
    }
}

fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), CliError> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        return print_help();
    };
    if matches!(command.as_str(), "-h" | "--help" | "help") {
        return print_help();
    }
    match command.as_str() {
        "cycles" | "cycles-turso" => cycles::run(arguments, command == "cycles-turso"),
        "ingest-extension" | "ingest-extension-turso" => {
            extensions::ingest(arguments, command == "ingest-extension-turso")
        }
        "init" => init_project(arguments),
        "watch" | "watch-turso" => watching::run(arguments, command == "watch-turso"),
        "index" => {
            let (root, snapshot, options) = index_arguments(arguments)?;
            index(&root, &snapshot, &options)
        }
        "index-turso" => {
            let root = arguments
                .next()
                .ok_or_else(|| CliError::Usage("index-turso requires a source root".to_owned()))?;
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("index-turso requires a database path".to_owned())
            })?;
            let options = index_options(arguments)?;
            index_turso(Path::new(&root), Path::new(&database), &options)
        }
        "search" => {
            let snapshot = arguments
                .next()
                .ok_or_else(|| CliError::Usage("search requires a snapshot path".to_owned()))?;
            let text = arguments
                .next()
                .ok_or_else(|| CliError::Usage("search requires text".to_owned()))?;
            search(Path::new(&snapshot), &text)
        }
        "node" => {
            let snapshot = arguments
                .next()
                .ok_or_else(|| CliError::Usage("node requires a snapshot path".to_owned()))?;
            let id = arguments
                .next()
                .ok_or_else(|| CliError::Usage("node requires a node ID".to_owned()))?;
            node(Path::new(&snapshot), parse_node_id(&id)?)
        }
        "node-turso" => {
            let database = arguments
                .next()
                .ok_or_else(|| CliError::Usage("node-turso requires a database path".to_owned()))?;
            let id = arguments
                .next()
                .ok_or_else(|| CliError::Usage("node-turso requires a node ID".to_owned()))?;
            node_turso(Path::new(&database), parse_node_id(&id)?)
        }
        "node-at" | "node-at-turso" => {
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let generation = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a generation ID")))?;
            let id = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a node ID")))?;
            let generation = parse_generation_id(&generation)?;
            let id = parse_node_id(&id)?;
            match command.as_str() {
                "node-at" => node_at(Path::new(&store_path), generation, id),
                "node-at-turso" => node_at_turso(Path::new(&store_path), generation, id),
                _ => Err(CliError::Usage(
                    "unsupported historical node command".to_owned(),
                )),
            }
        }
        "resolution-diagnostics" => {
            let snapshot = arguments.next().ok_or_else(|| {
                CliError::Usage("resolution-diagnostics requires a snapshot path".to_owned())
            })?;
            resolution_diagnostics(Path::new(&snapshot))
        }
        "resolution-diagnostics-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("resolution-diagnostics-turso requires a database path".to_owned())
            })?;
            resolution_diagnostics_turso(Path::new(&database))
        }
        "history" => {
            let snapshot = arguments
                .next()
                .ok_or_else(|| CliError::Usage("history requires a snapshot path".to_owned()))?;
            let id = arguments
                .next()
                .ok_or_else(|| CliError::Usage("history requires a node ID".to_owned()))?;
            history(Path::new(&snapshot), parse_node_id(&id)?)
        }
        "history-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("history-turso requires a database path".to_owned())
            })?;
            let id = arguments
                .next()
                .ok_or_else(|| CliError::Usage("history-turso requires a node ID".to_owned()))?;
            history_turso(Path::new(&database), parse_node_id(&id)?)
        }
        "fact-history" | "fact-history-turso" | "fact-lineage" | "fact-lineage-turso" => {
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let kind = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a fact kind")))?;
            let id = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a fact ID")))?;
            let fact = parse_fact_ref(&kind, &id)?;
            match command.as_str() {
                "fact-history" => fact_history(Path::new(&store_path), fact),
                "fact-history-turso" => fact_history_turso(Path::new(&store_path), fact),
                "fact-lineage" => fact_lineage(Path::new(&store_path), fact),
                "fact-lineage-turso" => fact_lineage_turso(Path::new(&store_path), fact),
                _ => Err(CliError::Usage(format!(
                    "unsupported fact query: {command}"
                ))),
            }
        }
        "graph-at" | "graph-at-turso" => {
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let generation = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a generation ID")))?;
            let generation = parse_generation_id(&generation)?;
            if command == "graph-at" {
                graph_at(Path::new(&store_path), generation)
            } else {
                graph_at_turso(Path::new(&store_path), generation)
            }
        }
        "graph-at-known-by" | "graph-at-known-by-turso" => {
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let generation = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a generation ID")))?;
            let accepted_by = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires accepted-by nanoseconds"))
            })?;
            let generation = parse_generation_id(&generation)?;
            let accepted_by = AcceptanceTime(accepted_by.parse().map_err(|error| {
                CliError::Usage(format!("invalid acceptance timestamp: {error}"))
            })?);
            if command == "graph-at-known-by" {
                graph_at_known_by(Path::new(&store_path), generation, accepted_by)
            } else {
                graph_at_known_by_turso(Path::new(&store_path), generation, accepted_by)
            }
        }
        "changes" | "changes-turso" => {
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let from = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a starting generation ID"))
            })?;
            let to = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires an ending generation ID"))
            })?;
            let from = parse_generation_id(&from)?;
            let to = parse_generation_id(&to)?;
            if command == "changes" {
                changes(Path::new(&store_path), from, to)
            } else {
                changes_turso(Path::new(&store_path), from, to)
            }
        }
        "change-set-at" | "change-set-at-turso" => {
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let change_set = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a ChangeSet ID")))?;
            let generation = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a generation ID")))?;
            let change_set = parse_change_set_id(&change_set)?;
            let generation = parse_generation_id(&generation)?;
            if command == "change-set-at" {
                change_set_at(Path::new(&store_path), change_set, generation)
            } else {
                change_set_at_turso(Path::new(&store_path), change_set, generation)
            }
        }
        "change-set-events" | "change-set-events-turso" => {
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let change_set = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a ChangeSet ID")))?;
            let generation = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a pinned generation ID"))
            })?;
            let limit = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a page limit")))?;
            let after = arguments.next();
            let change_set = parse_change_set_id(&change_set)?;
            let generation = parse_generation_id(&generation)?;
            let after = after.map(|value| parse_generation_id(&value)).transpose()?;
            let limit = parse_limit(&limit)?;
            if command == "change-set-events" {
                change_set_events(Path::new(&store_path), change_set, generation, after, limit)
            } else {
                change_set_events_turso(
                    Path::new(&store_path),
                    change_set,
                    generation,
                    after,
                    limit,
                )
            }
        }
        "change-correlations" | "change-correlations-turso" => {
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let source_generation = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a source generation ID"))
            })?;
            let fact_kind = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a fact kind")))?;
            let fact_id = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a fact ID")))?;
            let limit = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a page limit")))?;
            let after = arguments.next();
            let source_generation = parse_generation_id(&source_generation)?;
            let fact = parse_fact_ref(&fact_kind, &fact_id)?;
            let limit = parse_limit(&limit)?;
            let after = after.map(|value| parse_generation_id(&value)).transpose()?;
            if command == "change-correlations" {
                change_correlations(
                    Path::new(&store_path),
                    source_generation,
                    fact,
                    after,
                    limit,
                )
            } else {
                change_correlations_turso(
                    Path::new(&store_path),
                    source_generation,
                    fact,
                    after,
                    limit,
                )
            }
        }
        "accepted-between" | "accepted-between-turso" => {
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let from = arguments.next().ok_or_else(|| {
                CliError::Usage(format!(
                    "{command} requires a start Unix timestamp in nanoseconds"
                ))
            })?;
            let until = arguments.next().ok_or_else(|| {
                CliError::Usage(format!(
                    "{command} requires an exclusive end Unix timestamp in nanoseconds"
                ))
            })?;
            let limit = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a result limit")))?;
            let after_time = arguments.next();
            let after_generation = arguments.next();
            if after_time.is_some() != after_generation.is_some() {
                return Err(CliError::Usage(format!(
                    "{command} continuation requires both timestamp and generation ID"
                )));
            }
            let from = parse_acceptance_time(&from)?;
            let until = parse_acceptance_time(&until)?;
            let limit = parse_limit(&limit)?;
            let after = after_time
                .zip(after_generation)
                .map(|(time, generation)| {
                    Ok::<AcceptedGenerationCursor, CliError>(AcceptedGenerationCursor {
                        accepted_at: parse_acceptance_time(&time)?,
                        generation: parse_generation_id(&generation)?,
                    })
                })
                .transpose()?;
            if command == "accepted-between" {
                accepted_between(Path::new(&store_path), from, until, after, limit)
            } else {
                accepted_between_turso(Path::new(&store_path), from, until, after, limit)
            }
        }
        "observed-between" | "observed-between-turso" => {
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let from = arguments.next().ok_or_else(|| {
                CliError::Usage(format!(
                    "{command} requires an inclusive start observation time"
                ))
            })?;
            let until = arguments.next().ok_or_else(|| {
                CliError::Usage(format!(
                    "{command} requires an exclusive end observation time"
                ))
            })?;
            let limit = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a result limit")))?;
            let after_time = arguments.next();
            let after_kind = arguments.next();
            let after_id = arguments.next();
            let after_generation = arguments.next();
            let continuation = [
                after_time.as_ref(),
                after_kind.as_ref(),
                after_id.as_ref(),
                after_generation.as_ref(),
            ];
            if continuation.iter().any(Option::is_some) && continuation.iter().any(Option::is_none)
            {
                return Err(CliError::Usage(format!(
                    "{command} continuation requires time, kind, fact ID, and valid-from generation"
                )));
            }
            let from = parse_observation_time(&from)?;
            let until = parse_observation_time(&until)?;
            let limit = parse_limit(&limit)?;
            let after = after_time
                .zip(after_kind)
                .zip(after_id)
                .zip(after_generation)
                .map(|(((time, kind), id), generation)| {
                    Ok::<ObservedFactCursor, CliError>(ObservedFactCursor {
                        observed_at: parse_observation_time(&time)?,
                        fact: parse_fact_ref(&kind, &id)?,
                        valid_from: parse_generation_id(&generation)?,
                    })
                })
                .transpose()?;
            if command == "observed-between" {
                observed_between(Path::new(&store_path), from, until, after, limit)
            } else {
                observed_between_turso(Path::new(&store_path), from, until, after, limit)
            }
        }
        "neighbors" => {
            let snapshot = arguments
                .next()
                .ok_or_else(|| CliError::Usage("neighbors requires a snapshot path".to_owned()))?;
            let id = arguments
                .next()
                .ok_or_else(|| CliError::Usage("neighbors requires a node ID".to_owned()))?;
            neighbors(Path::new(&snapshot), parse_node_id(&id)?)
        }
        "neighbors-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("neighbors-turso requires a database path".to_owned())
            })?;
            let id = arguments
                .next()
                .ok_or_else(|| CliError::Usage("neighbors-turso requires a node ID".to_owned()))?;
            neighbors_turso(Path::new(&database), parse_node_id(&id)?)
        }
        "neighbors-at" | "neighbors-at-turso" => {
            let command = command.as_str();
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let generation = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a generation ID")))?;
            let endpoint = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a node ID")))?;
            let direction = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires incoming or outgoing direction"))
            })?;
            let limit = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a page limit")))?;
            let after = arguments
                .next()
                .map(|value| parse_edge_id(&value))
                .transpose()?;
            let path = Path::new(&store_path);
            let generation = parse_generation_id(&generation)?;
            let endpoint = parse_node_id(&endpoint)?;
            let direction = parse_edge_direction(&direction)?;
            let limit = parse_limit(&limit)?;
            if command == "neighbors-at" {
                historical_neighbors(path, generation, endpoint, direction, limit, after)
            } else {
                historical_neighbors_turso(path, generation, endpoint, direction, limit, after)
            }
        }
        "neighborhood-at" | "neighborhood-at-turso" => {
            let command = command.as_str();
            let store_path = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires a snapshot/database path"))
            })?;
            let generation = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a generation ID")))?;
            let seed_text = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires seed node IDs")))?;
            let seeds = seed_text
                .split(',')
                .map(parse_node_id)
                .collect::<Result<Vec<_>, _>>()?;
            let max_hops = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-hops")))?;
            let max_nodes = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-nodes")))?;
            let max_edges = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-edges")))?;
            let max_scanned_edges = arguments.next().ok_or_else(|| {
                CliError::Usage(format!("{command} requires max-scanned-incidences"))
            })?;
            let max_result_bytes = arguments
                .next()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-result-bytes")))?;
            let path = Path::new(&store_path);
            let generation = parse_generation_id(&generation)?;
            let max_hops = parse_limit(&max_hops)?;
            let max_nodes = parse_limit(&max_nodes)?;
            let max_edges = parse_limit(&max_edges)?;
            let max_scanned_edges = parse_limit(&max_scanned_edges)?;
            let max_result_bytes = parse_limit(&max_result_bytes)?;
            if command == "neighborhood-at" {
                historical_neighborhood(
                    path,
                    generation,
                    &seeds,
                    HistoricalNeighborhoodLimits {
                        max_hops,
                        max_nodes,
                        max_edges,
                        max_scanned_edges,
                        max_result_bytes,
                    },
                )
            } else {
                historical_neighborhood_turso(
                    path,
                    generation,
                    &seeds,
                    HistoricalNeighborhoodLimits {
                        max_hops,
                        max_nodes,
                        max_edges,
                        max_scanned_edges,
                        max_result_bytes,
                    },
                )
            }
        }
        "path" => {
            let snapshot = arguments
                .next()
                .ok_or_else(|| CliError::Usage("path requires a snapshot path".to_owned()))?;
            let start = arguments
                .next()
                .ok_or_else(|| CliError::Usage("path requires a start node ID".to_owned()))?;
            let target = arguments
                .next()
                .ok_or_else(|| CliError::Usage("path requires a target node ID".to_owned()))?;
            let max_hops = arguments
                .next()
                .ok_or_else(|| CliError::Usage("path requires a maximum hop count".to_owned()))?;
            path(
                Path::new(&snapshot),
                parse_node_id(&start)?,
                parse_node_id(&target)?,
                parse_limit(&max_hops)?,
            )
        }
        "path-turso" => {
            let database = arguments
                .next()
                .ok_or_else(|| CliError::Usage("path-turso requires a database path".to_owned()))?;
            let start = arguments
                .next()
                .ok_or_else(|| CliError::Usage("path-turso requires a start node ID".to_owned()))?;
            let target = arguments.next().ok_or_else(|| {
                CliError::Usage("path-turso requires a target node ID".to_owned())
            })?;
            let max_hops = arguments.next().ok_or_else(|| {
                CliError::Usage("path-turso requires a maximum hop count".to_owned())
            })?;
            path_turso(
                Path::new(&database),
                parse_node_id(&start)?,
                parse_node_id(&target)?,
                parse_limit(&max_hops)?,
            )
        }
        "subgraph" => {
            let snapshot = arguments
                .next()
                .ok_or_else(|| CliError::Usage("subgraph requires a snapshot path".to_owned()))?;
            let seed = arguments
                .next()
                .ok_or_else(|| CliError::Usage("subgraph requires a seed node ID".to_owned()))?;
            let max_hops = arguments.next().ok_or_else(|| {
                CliError::Usage("subgraph requires a maximum hop count".to_owned())
            })?;
            let max_nodes = arguments.next().ok_or_else(|| {
                CliError::Usage("subgraph requires a maximum node count".to_owned())
            })?;
            let max_edges = arguments.next().ok_or_else(|| {
                CliError::Usage("subgraph requires a maximum edge count".to_owned())
            })?;
            subgraph(
                Path::new(&snapshot),
                parse_node_id(&seed)?,
                parse_limit(&max_hops)?,
                parse_limit(&max_nodes)?,
                parse_limit(&max_edges)?,
            )
        }
        "subgraph-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("subgraph-turso requires a database path".to_owned())
            })?;
            let seed = arguments.next().ok_or_else(|| {
                CliError::Usage("subgraph-turso requires a seed node ID".to_owned())
            })?;
            let max_hops = arguments.next().ok_or_else(|| {
                CliError::Usage("subgraph-turso requires a maximum hop count".to_owned())
            })?;
            let max_nodes = arguments.next().ok_or_else(|| {
                CliError::Usage("subgraph-turso requires a maximum node count".to_owned())
            })?;
            let max_edges = arguments.next().ok_or_else(|| {
                CliError::Usage("subgraph-turso requires a maximum edge count".to_owned())
            })?;
            subgraph_turso(
                Path::new(&database),
                parse_node_id(&seed)?,
                parse_limit(&max_hops)?,
                parse_limit(&max_nodes)?,
                parse_limit(&max_edges)?,
            )
        }
        "consequence-neighborhood" | "consequence-neighborhood-turso" => {
            let mut values = arguments.collect::<VecDeque<_>>();
            let argument_count = values.len();
            if argument_count != 8 && argument_count != 10 {
                return Err(CliError::Usage(format!(
                    "{command} requires <store> <generation> <event <id>|fact <kind> <id> <valid-from>> <max-hops> <max-endpoints> <max-edges> <max-scanned-incidences>"
                )));
            }
            let store_path = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a store path")))?;
            let generation_text = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a generation")))?;
            let generation = parse_generation_id(&generation_text)?;
            let seed_kind = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a seed kind")))?;
            let seed = if seed_kind == "event" {
                if argument_count != 8 {
                    return Err(CliError::Usage("event seed form is event <id>".to_owned()));
                }
                let id = values
                    .pop_front()
                    .ok_or_else(|| CliError::Usage("event seed requires an ID".to_owned()))?;
                parse_consequence_seed("event", None, &id, None)?
            } else if seed_kind == "fact" {
                if argument_count != 10 {
                    return Err(CliError::Usage(
                        "fact seed form is fact <kind> <id> <valid-from>".to_owned(),
                    ));
                }
                let fact_kind = values
                    .pop_front()
                    .ok_or_else(|| CliError::Usage("fact seed requires a fact kind".to_owned()))?;
                let id = values
                    .pop_front()
                    .ok_or_else(|| CliError::Usage("fact seed requires an ID".to_owned()))?;
                let valid_from = values.pop_front().ok_or_else(|| {
                    CliError::Usage("fact seed requires a valid-from generation".to_owned())
                })?;
                parse_consequence_seed("fact", Some(&fact_kind), &id, Some(&valid_from))?
            } else {
                return Err(CliError::Usage("seed must be event or fact".to_owned()));
            };
            let max_hops = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-hops")))?;
            let max_endpoints = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-endpoints")))?;
            let max_edges = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-edges")))?;
            let max_scanned_incidences = values.pop_front().ok_or_else(|| {
                CliError::Usage(format!("{command} requires max-scanned-incidences"))
            })?;
            let max_hops = parse_limit(&max_hops)?;
            let max_endpoints = parse_limit(&max_endpoints)?;
            let max_edges = parse_limit(&max_edges)?;
            let max_scanned_incidences = parse_limit(&max_scanned_incidences)?;
            if command == "consequence-neighborhood" {
                consequence_neighborhood_file(
                    Path::new(&store_path),
                    generation,
                    seed,
                    max_hops,
                    max_endpoints,
                    max_edges,
                    max_scanned_incidences,
                )
            } else {
                consequence_neighborhood_turso(
                    Path::new(&store_path),
                    generation,
                    seed,
                    max_hops,
                    max_endpoints,
                    max_edges,
                    max_scanned_incidences,
                )
            }
        }
        "consequence-trace" | "consequence-trace-turso" => {
            let mut values = arguments.collect::<VecDeque<_>>();
            let argument_count = values.len();
            if argument_count != 10 && argument_count != 12 && argument_count != 14 {
                return Err(CliError::Usage(format!(
                    "{command} requires <store> <from-generation> <until-generation> <event <id>|fact <kind> <id> <valid-from>> <max-hops> <max-endpoints> <max-edges> <max-scanned-incidences> <max-result-bytes>"
                )));
            }
            let store_path = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a store path")))?;
            let from_generation = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires from-generation")))?;
            let until_generation = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires until-generation")))?;
            let from_generation = parse_generation_id(&from_generation)?;
            let until_generation = parse_generation_id(&until_generation)?;
            let seed_kind = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires a seed kind")))?;
            let seed = if seed_kind == "event" {
                if argument_count != 10 && argument_count != 12 {
                    return Err(CliError::Usage("event seed form is event <id>".to_owned()));
                }
                let id = values
                    .pop_front()
                    .ok_or_else(|| CliError::Usage("event seed requires an ID".to_owned()))?;
                parse_consequence_seed("event", None, &id, None)?
            } else if seed_kind == "fact" {
                if argument_count != 12 && argument_count != 14 {
                    return Err(CliError::Usage(
                        "fact seed form is fact <kind> <id> <valid-from>".to_owned(),
                    ));
                }
                let fact_kind = values
                    .pop_front()
                    .ok_or_else(|| CliError::Usage("fact seed requires a fact kind".to_owned()))?;
                let id = values
                    .pop_front()
                    .ok_or_else(|| CliError::Usage("fact seed requires an ID".to_owned()))?;
                let valid_from = values.pop_front().ok_or_else(|| {
                    CliError::Usage("fact seed requires a valid-from generation".to_owned())
                })?;
                parse_consequence_seed("fact", Some(&fact_kind), &id, Some(&valid_from))?
            } else {
                return Err(CliError::Usage("seed must be event or fact".to_owned()));
            };
            let max_hops = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-hops")))?;
            let max_endpoints = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-endpoints")))?;
            let max_edges = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-edges")))?;
            let max_scanned_incidences = values.pop_front().ok_or_else(|| {
                CliError::Usage(format!("{command} requires max-scanned-incidences"))
            })?;
            let max_result_bytes = values
                .pop_front()
                .ok_or_else(|| CliError::Usage(format!("{command} requires max-result-bytes")))?;
            let included_evidence_classes = if let Some(option) = values.pop_front() {
                if option != "--evidence-classes" {
                    return Err(CliError::Usage(format!(
                        "unexpected option {option:?}; expected --evidence-classes"
                    )));
                }
                let classes = values.pop_front().ok_or_else(|| {
                    CliError::Usage("--evidence-classes requires a comma-separated set".to_owned())
                })?;
                if !values.is_empty() {
                    return Err(CliError::Usage(
                        "unexpected extra consequence-trace arguments".to_owned(),
                    ));
                }
                parse_evidence_classes(&classes)?
            } else {
                Vec::new()
            };
            let request = ConsequenceTraceRequest {
                origin: seed,
                from_generation,
                until_generation,
                max_hops: parse_limit(&max_hops)?,
                max_endpoints: parse_limit(&max_endpoints)?,
                max_edges: parse_limit(&max_edges)?,
                max_scanned_incidences: parse_limit(&max_scanned_incidences)?,
                included_kinds: Vec::new(),
                included_evidence_classes,
            };
            let max_result_bytes = parse_limit(&max_result_bytes)?;
            if command == "consequence-trace" {
                consequence_trace_file(Path::new(&store_path), &request, max_result_bytes)
            } else {
                consequence_trace_turso(Path::new(&store_path), &request, max_result_bytes)
            }
        }
        "impact" => {
            let snapshot = arguments
                .next()
                .ok_or_else(|| CliError::Usage("impact requires a snapshot path".to_owned()))?;
            let text = arguments
                .next()
                .ok_or_else(|| CliError::Usage("impact requires text".to_owned()))?;
            impact(Path::new(&snapshot), &text)
        }
        "search-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("search-turso requires a database path".to_owned())
            })?;
            let text = arguments
                .next()
                .ok_or_else(|| CliError::Usage("search-turso requires text".to_owned()))?;
            search_turso(Path::new(&database), &text)
        }
        "impact-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("impact-turso requires a database path".to_owned())
            })?;
            let text = arguments
                .next()
                .ok_or_else(|| CliError::Usage("impact-turso requires text".to_owned()))?;
            impact_turso(Path::new(&database), &text)
        }
        "export" => {
            let snapshot = arguments
                .next()
                .ok_or_else(|| CliError::Usage("export requires a snapshot path".to_owned()))?;
            export(Path::new(&snapshot))
        }
        "export-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("export-turso requires a database path".to_owned())
            })?;
            export_turso(Path::new(&database))
        }
        "status" => {
            let snapshot = arguments
                .next()
                .ok_or_else(|| CliError::Usage("status requires a snapshot path".to_owned()))?;
            let source_root = arguments.next();
            status(Path::new(&snapshot), source_root.as_deref().map(Path::new))
        }
        "processing-coverage" => processing::run(arguments, false),
        "processing-coverage-turso" => processing::run(arguments, true),
        "status-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("status-turso requires a database path".to_owned())
            })?;
            let source_root = arguments.next();
            status_turso(Path::new(&database), source_root.as_deref().map(Path::new))
        }
        "workflow-rejections" => {
            let snapshot = arguments.next().ok_or_else(|| {
                CliError::Usage("workflow-rejections requires a snapshot path".to_owned())
            })?;
            let limit = arguments
                .next()
                .ok_or_else(|| CliError::Usage("workflow-rejections requires a limit".to_owned()))
                .and_then(|value| parse_limit(&value))?;
            let after = arguments
                .next()
                .as_deref()
                .map(parse_index_run_id)
                .transpose()?;
            workflow_rejections(Path::new(&snapshot), limit, after)
        }
        "workflow-rejections-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("workflow-rejections-turso requires a database path".to_owned())
            })?;
            let limit = arguments
                .next()
                .ok_or_else(|| {
                    CliError::Usage("workflow-rejections-turso requires a limit".to_owned())
                })
                .and_then(|value| parse_limit(&value))?;
            let after = arguments
                .next()
                .as_deref()
                .map(parse_index_run_id)
                .transpose()?;
            workflow_rejections_turso(Path::new(&database), limit, after)
        }
        "git-context" => {
            let root = arguments
                .next()
                .ok_or_else(|| CliError::Usage("git-context requires a source root".to_owned()))?;
            git_context(Path::new(&root))
        }
        "integrity" => {
            let snapshot = arguments
                .next()
                .ok_or_else(|| CliError::Usage("integrity requires a snapshot path".to_owned()))?;
            integrity_snapshot(Path::new(&snapshot))
        }
        "integrity-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("integrity-turso requires a database path".to_owned())
            })?;
            integrity_turso(Path::new(&database))
        }
        "statechronicle-verify" => {
            let snapshot = arguments.next().ok_or_else(|| {
                CliError::Usage("statechronicle-verify requires a snapshot path".to_owned())
            })?;
            verify_statechronicle_file(Path::new(&snapshot))
        }
        "statechronicle-verify-turso" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("statechronicle-verify-turso requires a database path".to_owned())
            })?;
            verify_statechronicle_turso(Path::new(&database))
        }
        "turso-migrate" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("turso-migrate requires a database path".to_owned())
            })?;
            turso_migrate(Path::new(&database))
        }
        "turso-migration-status" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("turso-migration-status requires a database path".to_owned())
            })?;
            turso_migration_status(Path::new(&database))
        }
        "sqlite-migrate" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("sqlite-migrate requires a database path".to_owned())
            })?;
            sqlite_migrate(Path::new(&database))
        }
        "sqlite-migration-status" => {
            let database = arguments.next().ok_or_else(|| {
                CliError::Usage("sqlite-migration-status requires a database path".to_owned())
            })?;
            sqlite_migration_status(Path::new(&database))
        }
        other => Err(CliError::Usage(format!(
            "unknown command: {other}; run `syntaxmesh --help` to see supported commands"
        ))),
    }
}

pub(super) fn run_cli() {
    if let Err(error) = run(std::env::args().skip(1)) {
        eprintln!("syntaxmesh: {error}");
        std::process::exit(2);
    }
}
