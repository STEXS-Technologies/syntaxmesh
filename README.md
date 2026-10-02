# SyntaxMesh

## Source, documentation, and history intelligence

SyntaxMesh builds a local, provenance-backed graph of your repository so you
can search code, explore relationships, understand documentation, and inspect
changes across retained generations.

It runs entirely in Rust and analyzes Rust, TypeScript/JavaScript, Python, Bash,
Markdown, and text without launching their language runtimes.

## Start locally

Use the pinned Rust toolchain. Keep storage outside the directory being indexed.

```bash
git clone https://github.com/STEXS-Technologies/syntaxmesh.git
cd syntaxmesh
cargo build --locked -p syntaxmesh-cli
target/debug/syntaxmesh index /path/to/source /path/to/state/graph.snapshot
target/debug/syntaxmesh --help
```

See [Usage](docs/USAGE.md) for queries, history, MCP, and optional AI setup,
and [Operations](docs/OPERATIONS.md) for migrations and recovery.

## Status

SyntaxMesh is an early v0 implementation. Basic indexing, querying, durable
workflows, and retained history work; corpus-scale retrieval quality and higher
reasoning layers remain in development. Extraction is not full compiler
analysis, and unresolved references stay explicit.

See the [roadmap](docs/V0_ARCHITECTURE_PLAN.md) for scope and architecture.

## Contributing

Run `cargo make ci` and follow [CONTRIBUTING.md](CONTRIBUTING.md).
