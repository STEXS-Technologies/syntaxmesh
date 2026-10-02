use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMacro, LitStr};

mod context_bench;
mod extension_process;
mod fuzz_smoke;
mod repository_bench;
mod write_bench;

type TaskResult<T> = Result<T, Box<dyn Error>>;

fn main() -> TaskResult<()> {
    let task = env::args().nth(1).unwrap_or_default();
    let root = workspace_root()?;
    match task.as_str() {
        "architecture" => architecture(&root),
        "extension-fixture" => extension_fixture(&root),
        "turso-fts-probe" => turso_fts_probe(&root),
        "sqlite-migrations" => migrations(&root, MigrationBackend::Sqlite),
        "turso-migrations" => migrations(&root, MigrationBackend::Turso),
        "benchmark-file-writes" => write_bench::run(&root),
        "benchmark-context-retrieval" => context_bench::run(&root),
        "benchmark-repository-evidence" => repository_bench::run(&root),
        "fuzz-source-extractors" => fuzz_smoke::run(&root),
        _ => Err(format!("usage: cargo run -p syntaxmesh-xtask -- <architecture|extension-fixture|turso-fts-probe|sqlite-migrations|turso-migrations|benchmark-context-retrieval|benchmark-repository-evidence|benchmark-file-writes|fuzz-source-extractors>; got {task:?}").into()),
    }
}

fn turso_fts_probe(root: &Path) -> TaskResult<()> {
    let manifest = root.join("fixtures/turso-fts-probe/Cargo.toml");
    let output = Command::new("cargo")
        .current_dir(root)
        .arg("run")
        .arg("--locked")
        .arg("--manifest-path")
        .arg(&manifest)
        .output()?;
    if !output.status.success() {
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return Err("pinned Turso FTS capability probe failed".into());
    }
    print!("{}", String::from_utf8(output.stdout)?);
    Ok(())
}

fn extension_fixture(root: &Path) -> TaskResult<()> {
    let manifest = root.join("fixtures/out-of-tree-extension/Cargo.toml");
    let output = Command::new("cargo")
        .current_dir(root)
        .arg("run")
        .arg("--locked")
        .arg("--manifest-path")
        .arg(&manifest)
        .output()?;
    if !output.status.success() {
        eprint!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return Err("out-of-tree extension consumer fixture failed".into());
    }
    print!("{}", String::from_utf8(output.stdout)?);
    extension_process::verify(root, &manifest)
}

fn workspace_root() -> TaskResult<PathBuf> {
    let output = Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .output()?;
    if !output.status.success() {
        return Err("cargo locate-project failed".into());
    }
    let manifest = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    Ok(manifest
        .parent()
        .ok_or("workspace manifest has no parent")?
        .to_owned())
}

fn architecture(root: &Path) -> TaskResult<()> {
    let output = Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .output()?;
    if !output.status.success() {
        return Err("cargo metadata failed".into());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let packages: BTreeMap<&str, &serde_json::Value> = metadata
        .get("packages")
        .ok_or("cargo metadata omitted packages")?
        .as_array()
        .ok_or("cargo metadata omitted packages")?
        .iter()
        .filter_map(|package| Some((json_string_field(package, "name")?, package)))
        .collect();
    if packages.contains_key("syntaxmesh-fuzz") {
        return Err("fuzz-only package leaked into the stable workspace".into());
    }
    let pure = [
        "syntaxmesh-core",
        "syntaxmesh-api-model",
        "syntaxmesh-runtime-protocol",
        "syntaxmesh-resolver",
    ];
    let neutral = [
        "syntaxmesh-core",
        "syntaxmesh-api-model",
        "syntaxmesh-runtime-protocol",
        "syntaxmesh-query",
        "syntaxmesh-indexer",
        "syntaxmesh-engine",
    ];
    let forbidden = [
        "turso",
        "sqlite",
        "duckdb",
        "penelope",
        "statechronicle",
        "tokio",
        "tree-sitter",
        "axum",
        "hyper",
        "syntaxmesh-store",
        "syntaxmesh-engine",
        "syntaxmesh-daemon",
        "syntaxmesh-http",
        "syntaxmesh-mcp",
    ];
    let mut violations = Vec::new();

    for source in rust_files(&root.join("crates"))? {
        let text = fs::read_to_string(&source)?;
        let syntax = syn::parse_file(&text).map_err(|error| {
            format!(
                "could not parse Rust source for architecture audit at {}: {error}",
                source.display()
            )
        })?;
        let mut launch_audit = RuntimeLaunchAudit {
            source: source.display().to_string(),
            violations: Vec::new(),
        };
        launch_audit.visit_file(&syntax);
        violations.extend(launch_audit.violations);
        if text.contains("#[allow") || text.contains("#[expect") {
            violations.push(format!(
                "{} must not contain allow/expect attributes",
                source.display()
            ));
        }
        if source
            .file_name()
            .is_some_and(|name| name == "lib.rs" || name == "mod.rs")
        {
            for (line_no, line) in text.lines().enumerate() {
                let stripped = line.trim_start();
                let declaration = [
                    "struct ",
                    "enum ",
                    "trait ",
                    "fn ",
                    "const ",
                    "static ",
                    "type ",
                    "macro_rules!",
                ]
                .iter()
                .any(|prefix| {
                    stripped.starts_with(prefix)
                        || stripped.starts_with(&format!("pub {prefix}"))
                        || stripped.starts_with(&format!("pub(crate) {prefix}"))
                });
                if declaration || stripped.starts_with("impl ") || stripped.starts_with("pub impl ")
                {
                    violations.push(format!(
                        "{}:{} puts implementation in a crate/module root",
                        source.display(),
                        line_no.saturating_add(1)
                    ));
                }
            }
        }
    }

    for name in pure {
        let Some(package) = packages.get(name) else {
            violations.push(format!("missing required pure crate: {name}"));
            continue;
        };
        for dependency in json_array_field(package, "dependencies") {
            let dependency_name = json_string_field(dependency, "name").unwrap_or_default();
            if forbidden.iter().any(|part| dependency_name.contains(part)) {
                violations.push(format!("{name} must not depend on {dependency_name}"));
            }
        }
    }
    for name in neutral {
        if let Some(package) = packages.get(name) {
            for dependency in json_array_field(package, "dependencies") {
                let dependency_name = json_string_field(dependency, "name").unwrap_or_default();
                if ["rmcp", "tokio"].contains(&dependency_name)
                    && json_field(dependency, "kind").is_none_or(serde_json::Value::is_null)
                {
                    violations.push(format!(
                        "{name} must remain independent of {dependency_name}"
                    ));
                }
            }
        }
    }
    require_dependencies(
        &packages,
        "syntaxmesh-mcp",
        &[
            "rmcp",
            "tokio",
            "syntaxmesh-engine",
            "syntaxmesh-query",
            "syntaxmesh-store-turso",
        ],
        &mut violations,
    );
    require_dependencies(
        &packages,
        "syntaxmesh-http",
        &[
            "axum",
            "tokio",
            "syntaxmesh-engine",
            "syntaxmesh-query",
            "syntaxmesh-store-turso",
        ],
        &mut violations,
    );
    require_dependencies(
        &packages,
        "syntaxmesh-engine",
        &[
            "syntaxmesh-integration-penelope",
            "syntaxmesh-integration-statechronicle",
        ],
        &mut violations,
    );

    if violations.is_empty() {
        println!("Architecture dependency boundaries: OK");
        Ok(())
    } else {
        for violation in violations {
            eprintln!("architecture violation: {violation}");
        }
        Err("architecture check failed".into())
    }
}

struct RuntimeLaunchAudit {
    source: String,
    violations: Vec<String>,
}

impl<'ast> Visit<'ast> for RuntimeLaunchAudit {
    fn visit_expr_call(&mut self, expression: &'ast ExprCall) {
        if is_command_constructor(&expression.func) {
            let executable = expression.args.first().and_then(command_executable);
            let rust_test_self = self.source.contains("/tests/")
                && expression.args.first().is_some_and(|argument| {
                    let Expr::Try(tried) = argument else {
                        return false;
                    };
                    let Expr::Call(call) = tried.expr.as_ref() else {
                        return false;
                    };
                    let Expr::Path(path) = call.func.as_ref() else {
                        return false;
                    };
                    call.args.is_empty()
                        && path
                            .path
                            .segments
                            .iter()
                            .map(|segment| segment.ident.to_string())
                            .collect::<Vec<_>>()
                            == ["std", "env", "current_exe"]
                });
            // Explicit opt-in AI harness transport is the sole dynamic host exception.
            let semantic_harness =
                self.source.ends_with("/cli/semantic/command.rs") && executable.is_none();
            if !rust_test_self
                && !semantic_harness
                && !executable
                    .is_some_and(|name| name == "git" || name.starts_with("CARGO_BIN_EXE_"))
            {
                self.violations.push(format!(
                    "{} launches a non-Rust process; source runtimes must not be executed",
                    self.source
                ));
            }
        }
        syn::visit::visit_expr_call(self, expression);
    }
}

fn is_command_constructor(expression: &Expr) -> bool {
    let Expr::Path(path) = expression else {
        return false;
    };
    let mut segments = path.path.segments.iter().rev();
    segments
        .next()
        .is_some_and(|segment| segment.ident == "new")
        && segments
            .next()
            .is_some_and(|segment| segment.ident == "Command")
}

fn command_executable(expression: &Expr) -> Option<String> {
    if let Expr::Lit(literal) = expression
        && let syn::Lit::Str(value) = &literal.lit
    {
        return Some(value.value());
    }
    let Expr::Macro(ExprMacro { mac, .. }) = expression else {
        return None;
    };
    if !mac.path.is_ident("env") {
        return None;
    }
    syn::parse2::<LitStr>(mac.tokens.clone())
        .ok()
        .map(|value| value.value())
}

#[cfg(test)]
mod architecture_tests {
    use super::RuntimeLaunchAudit;
    use syn::visit::Visit;

    fn launch_violations(source: &str) -> Vec<String> {
        let Ok(syntax) = syn::parse_file(source) else {
            return vec!["invalid Rust test source".to_owned()];
        };
        let mut audit = RuntimeLaunchAudit {
            source: "test.rs".to_owned(),
            violations: Vec::new(),
        };
        audit.visit_file(&syntax);
        audit.violations
    }

    #[test]
    fn test_binaries_can_relaunch_only_the_exact_rust_self_executable()
    -> Result<(), Box<dyn std::error::Error>> {
        let syntax = syn::parse_file(
            r#"fn test() { let _child = std::process::Command::new(std::env::current_exe()?); }"#,
        )?;
        let mut audit = RuntimeLaunchAudit {
            source: "crate/tests/live.rs".to_owned(),
            violations: Vec::new(),
        };
        audit.visit_file(&syntax);
        if !audit.violations.is_empty() || launch_violations(r#"fn production() { let _child = std::process::Command::new(std::env::current_exe()?); }"#).len() != 1 {
            return Err(std::io::Error::other("Rust test self-launch boundary mismatch").into());
        }
        Ok(())
    }

    #[test]
    fn process_launches_allow_only_git_and_rust_workspace_binaries() {
        let source = r#"
            fn allowed() {
                let _git = std::process::Command::new("git");
                let _engine = std::process::Command::new(env!("CARGO_BIN_EXE_syntaxmesh"));
            }
        "#;
        assert!(launch_violations(source).is_empty());
    }

    #[test]
    fn dynamic_harness_exception_is_scoped_to_command_adapter()
    -> Result<(), Box<dyn std::error::Error>> {
        let syntax = syn::parse_file(
            r#"fn run(executable: &str) {
                let _configured = std::process::Command::new(executable);
                let _forbidden = std::process::Command::new("python3");
            }"#,
        )?;
        for (source, expected) in [
            ("crates/syntaxmesh-cli/src/cli/semantic/command.rs", 1),
            ("crates/syntaxmesh-cli/src/cli/semantic.rs", 2),
            ("crates/syntaxmesh-engine/src/command.rs", 2),
        ] {
            let mut audit = RuntimeLaunchAudit {
                source: source.to_owned(),
                violations: Vec::new(),
            };
            audit.visit_file(&syntax);
            if audit.violations.len() != expected {
                return Err("semantic harness runtime boundary mismatch".into());
            }
        }
        Ok(())
    }

    #[test]
    fn process_launches_reject_analyzed_language_runtimes_and_dynamic_names() {
        let source = r#"
            fn rejected(runtime: &str) {
                let _python = std::process::Command::new("python3");
                let _dynamic = std::process::Command::new(runtime);
            }
        "#;
        assert_eq!(launch_violations(source).len(), 2);
    }

    #[test]
    fn command_examples_in_comments_do_not_trigger_runtime_audit() {
        assert!(launch_violations("// Command::new(\"python3\")\nfn analyze() {}\n").is_empty());
    }
}

fn require_dependencies(
    packages: &BTreeMap<&str, &serde_json::Value>,
    package_name: &str,
    required: &[&str],
    violations: &mut Vec<String>,
) {
    let Some(package) = packages.get(package_name) else {
        return;
    };
    let dependencies: BTreeSet<&str> = json_array_field(package, "dependencies")
        .into_iter()
        .filter(|dep| json_field(dep, "kind").is_none_or(serde_json::Value::is_null))
        .filter_map(|dep| json_string_field(dep, "name"))
        .collect();
    for name in required {
        if !dependencies.contains(name) {
            violations.push(format!("{package_name} requires {name}"));
        }
    }
}

fn json_field<'value>(
    value: &'value serde_json::Value,
    name: &str,
) -> Option<&'value serde_json::Value> {
    value.get(name)
}

fn json_array_field<'value>(
    value: &'value serde_json::Value,
    name: &str,
) -> Vec<&'value serde_json::Value> {
    json_field(value, name)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .collect()
}

fn json_string_field<'value>(value: &'value serde_json::Value, name: &str) -> Option<&'value str> {
    json_field(value, name).and_then(serde_json::Value::as_str)
}

fn rust_files(root: &Path) -> TaskResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(rust_files(&path)?);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(files)
}

#[derive(Clone, Copy)]
enum MigrationBackend {
    Sqlite,
    Turso,
}

fn migrations(root: &Path, backend: MigrationBackend) -> TaskResult<()> {
    let (label, registry, adapter, directory) = match backend {
        MigrationBackend::Sqlite => (
            "SQLite",
            "crates/syntaxmesh-store-sqlite/src/backend/migrations.rs",
            "crates/syntaxmesh-store-sqlite/src/backend.rs",
            "crates/syntaxmesh-store-sqlite/migrations",
        ),
        MigrationBackend::Turso => (
            "Turso",
            "crates/syntaxmesh-store-turso/src/migrations.rs",
            "crates/syntaxmesh-store-turso/src/adapter.rs",
            "crates/syntaxmesh-store-turso/migrations",
        ),
    };
    let registry_text = fs::read_to_string(root.join(registry))?;
    let adapter_text = fs::read_to_string(root.join(adapter))?;
    let version_re = regex::Regex::new(r"const SCHEMA_VERSION:\s*i64\s*=\s*(\d+)")?;
    let latest: i64 = version_re
        .captures(&adapter_text)
        .and_then(|c| c[1].parse().ok())
        .ok_or("missing schema version")?;
    let entry_re = regex::Regex::new(
        r#"SchemaMigration\s*\{\s*from:\s*(\d+),\s*to:\s*(\d+),\s*name:\s*"([^"]+)""#,
    )?;
    let entries: Vec<(i64, i64, String)> = entry_re
        .captures_iter(&registry_text)
        .map(|c| Ok((c[1].parse()?, c[2].parse()?, c[3].to_owned())))
        .collect::<TaskResult<_>>()?;
    let expected: Vec<(i64, i64)> = (1..latest)
        .filter_map(|version| version.checked_add(1).map(|next| (version, next)))
        .collect();
    let actual: Vec<(i64, i64)> = entries.iter().map(|(from, to, _)| (*from, *to)).collect();
    if entries.is_empty() || actual != expected {
        return Err(format!("{label} migration registry is not contiguous through schema version {latest}: {actual:?}").into());
    }

    let migration_dir = root.join(directory);
    let up_paths = sql_paths(&migration_dir, ".up.sql")?;
    let up_names = file_names(&up_paths)?;
    let ref_re = if matches!(backend, MigrationBackend::Sqlite) {
        regex::Regex::new(r#"include_str!\(\s*"(?:\.\./)+migrations/([^"/]+\.up\.sql)"\s*\)"#)?
    } else {
        regex::Regex::new(r#"include_str!\(\s*"\.\./migrations/(\d{4}_[^"/]+\.up\.sql)"\s*\)"#)?
    };
    let references: BTreeSet<String> = ref_re
        .captures_iter(&registry_text)
        .map(|c| c[1].to_owned())
        .collect();
    let filtered_up: BTreeSet<String> = up_names
        .iter()
        .filter(|name| {
            matches!(backend, MigrationBackend::Turso) || name.as_str() != "0000_initial_schema.sql"
        })
        .cloned()
        .collect();
    if filtered_up != references {
        return Err(format!("{label} SQL files and registry references differ: files={filtered_up:?}, references={references:?}").into());
    }

    if matches!(backend, MigrationBackend::Sqlite) {
        validate_sqlite_registry(&registry_text, &migration_dir, &filtered_up)?;
    }

    let filename_re = regex::Regex::new(r"(\d{4})_(.+)\.up\.sql")?;
    for filename in &filtered_up {
        let captures = filename_re
            .captures(filename)
            .ok_or_else(|| format!("malformed {label} migration filename {filename}"))?;
        let version: i64 = captures[1].parse()?;
        let name = captures[2].to_owned();
        if !entries
            .iter()
            .any(|(_, to, registered_name)| *to == version && registered_name == &name)
        {
            return Err(format!("{label} migration {filename} disagrees with registry").into());
        }
    }
    println!(
        "{label} migration files match registry ({} SQL-backed, {} Rust-only)",
        filtered_up.len(),
        entries.len().saturating_sub(filtered_up.len())
    );
    Ok(())
}

fn sql_paths(directory: &Path, suffix: &str) -> TaskResult<Vec<PathBuf>> {
    Ok(fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(suffix))
        })
        .collect())
}

fn validate_sqlite_registry(
    registry: &str,
    directory: &Path,
    sql_migrations: &BTreeSet<String>,
) -> TaskResult<()> {
    let include_set = |pattern: &str| -> TaskResult<BTreeSet<String>> {
        let regex = regex::Regex::new(pattern)?;
        Ok(regex
            .captures_iter(registry)
            .map(|captures| captures[1].to_owned())
            .collect())
    };
    let checksum = include_set(
        r#"checksum_sql:\s*Some\(include_str!\(\s*"(?:\.\./)+migrations/([^"/]+\.up\.sql)"\s*\)\s*\)"#,
    )?;
    let bootstrap = include_set(
        r#"fresh_bootstrap_sql:\s*Some\(include_str!\(\s*"(?:\.\./)+migrations/([^"/]+\.up\.sql)"\s*\)\s*\)"#,
    )?;
    if &checksum != sql_migrations {
        return Err(format!("SQLite checksum SQL does not cover exactly the registered SQL migrations: checksummed={checksum:?}, migrations={sql_migrations:?}").into());
    }
    if !bootstrap.is_subset(&checksum) {
        return Err(format!(
            "SQLite fresh-bootstrap SQL must use checksummed migrations: {bootstrap:?}"
        )
        .into());
    }

    let down_files = file_names(&sql_paths(directory, ".down.sql")?)?;
    let rollback_regex = regex::Regex::new(
        r#"rollback_sql:\s*Some\(include_str!\(\s*"(?:\.\./)+migrations/([^"/]+\.down\.sql)"\s*\)\s*\)"#,
    )?;
    let rollback_refs: BTreeSet<String> = rollback_regex
        .captures_iter(registry)
        .map(|capture| capture[1].to_owned())
        .collect();
    if rollback_refs != down_files {
        return Err(format!(
            "SQLite rollback registry mismatch: referenced={rollback_refs:?}, files={down_files:?}"
        )
        .into());
    }

    let filename_regex = regex::Regex::new(r"^(\d{4})_(.+)\.up\.sql$")?;
    let mut registered_sql = BTreeSet::new();
    for filename in sql_migrations {
        let capture = filename_regex
            .captures(filename)
            .ok_or_else(|| format!("malformed SQLite migration filename {filename}"))?;
        registered_sql.insert((capture[1].parse::<i64>()?, capture[2].to_owned()));
    }
    if registered_sql.len() != sql_migrations.len() {
        return Err("duplicate SQLite migration version/name".into());
    }
    Ok(())
}

fn file_names(paths: &[PathBuf]) -> TaskResult<BTreeSet<String>> {
    paths
        .iter()
        .map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .ok_or_else(|| format!("path has no filename: {}", path.display()).into())
        })
        .collect()
}
