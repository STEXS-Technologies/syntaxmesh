# ADR-0166: Conservative ECMAScript `this` receiver scope

- Status: accepted
- Date: 2026-09-30

## Context

The Oxc extractor currently qualifies `this.method()` using every enclosing
class name. An ordinary function or object method nested in a class method has
its own receiver, however; resolving it to the enclosing class fabricates a
call edge. Nested class names also omit intervening lexical scopes, unlike
their actual declared graph names. Sim's existing class-method arrow callbacks
illustrate why arrows must preserve their enclosing `this` context.

## Decision

Reuse Oxc class/function boundaries and the extractor's existing enter/leave
frames. Track the current named class's full declaration scope separately from
the method's `this` context. A class method's function body establishes that
class context. Ordinary functions, object methods, and anonymous classes do not
inherit it. Arrows inherit the current context; leaving a function or class
restores the previous context. Nested named classes use their full existing
declaration path, not a second class-only naming convention.

Only qualify existing static `this.member()` targets when this explicit method
context exists. Otherwise preserve the original call spelling and evidence as
an unresolved occurrence through the unchanged conservative resolver. Class
field initializers and computed class keys remain unqualified, as do dynamic
receivers, indirect calls, inheritance dispatch, and explicit receiver rebinding.
This is syntax-level receiver context, not proof of runtime execution or
compiler/type-checker equivalence.

Bump ECMAScript extractor semantics from 9 to 10 through existing producer
invalidation. No public contract, resolver, dependency, database schema,
workflow, transport, or implementation runtime is added. Dedicated child tests
follow the adopted Shardline layout; durable indexing and history keep using
the existing Penelope/store paths.

## Verification

The regression fixture failed on the prior extractor and now passes for both
TypeScript and JavaScript. It covers ordinary and object functions, lexically
inheriting arrows, anonymous classes, class fields, nested named classes, scope
restoration, exact evidence, and conservative call edges. A separate computed
class-key fixture verifies that key evaluation does not assume the new class's
receiver. All 19 ECMAScript tests pass.

The reused File/verified-Turso CLI host helper exercises both languages through
Penelope indexing. An unchanged repeat reuses the generation; replacing lexical
arrows with ordinary functions retracts current call edges. Reopened retained
history preserves the exact original nodes and edges. Both lifecycle tests
pass, including the explicit StateChronicle verification status check.

`cargo make ci` completed successfully in 117.42 seconds: formatting,
architecture boundaries, out-of-tree extension, migration registries,
all-feature compilation and strict Clippy, dependency checks, workspace tests,
and API documentation. Existing policy-approved dependency warnings remain;
no new exception was added. Broad language-quality and release gates remain
open; this does not validate runtime dispatch or compiler-equivalent semantics.

An isolated upgrade of the version-9 Sim index accepted all 2,572 files at the
clean revision `63f18995da8c7b3c458546980008db75714ec1c2`. All 2,548 TypeScript
and two JavaScript producer records use extractor version 10. The graph still
has 87,486 nodes / 89,767 edges; its 4,586 direct call edges, 5,592 resolved call
occurrences, 45,439 unresolved call occurrences, and 1,594 ambiguous call
occurrences are unchanged. This proves corpus compatibility, not observed
precision improvement in that corpus. Focused fixtures establish the corrected
cases. Reopened freshness/root/reference/File-store integrity checks passed,
and an unchanged repeat reused generation
`b34884aef4ab1ef965baec06966ecc6f4dc7bc713064fdbdffeeaba517a6da78`.

The retained pre-upgrade node/edge/provenance export matches the original
version-9 index after omitting top-level query-mode fields; both SHA-256 values
are `34e4556fbd3a3e68de9c179c156680de7326eb498c7b9c4883fb40b56cb41cb8`.
The Sim worktree and version-9 index were not modified. The isolated index and
observations are retained under ignored
`target/validation/sim-receiver-v10-20260930-y0q7H1/`. No AI inference,
module-resolution profile, or latency comparison was exercised.
