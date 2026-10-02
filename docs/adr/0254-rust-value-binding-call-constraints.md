# ADR 0254: Rust value-binding call constraints

Status: accepted

Rust syntax extraction tracks parameter, local, closure, loop, match, and
conditional pattern bindings with lexical scope frames. Initializers are visited
before installing local bindings. Bare calls through these bindings retain
their spelling and source evidence but use the SDK Unresolved constraint.
Qualified paths keep the existing resolution policy. Nested named declarations
remain separately extracted; compiler-grade enclosing-item/module resolution,
macro expansion, and actual callable-value target inference remain open.
