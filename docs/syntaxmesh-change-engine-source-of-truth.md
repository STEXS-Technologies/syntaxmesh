# SyntaxMesh Change Engine — Source of Truth v1

> Working name: **SyntaxMesh Change Engine** (project/product name intentionally not frozen).
>
> Purpose: define a separate transformation and verification system that sits above SyntaxMesh. It converts an engineering intent plus SyntaxMesh evidence into an executable, reviewable change plan; applies repository/config/schema transformations through bounded providers; repeatedly re-indexes the changed system with SyntaxMesh; and verifies that the requested semantic postconditions, contracts, and safety constraints hold.
>
> Date: 2026-09-23.
>
> Status: architectural source of truth for a future project. This is deliberately **not** part of SyntaxMesh core.

---

# 0. Core thesis

SyntaxMesh should answer:

```text
What exists?
What does it mean?
Why do we believe it?
What depends on it?
What contracts apply?
What must remain true if it changes?
How will we know the change is correct?
```

The Change Engine should answer:

```text
Given that evidence and intent,
what ordered work must be performed,
how can specialized transformation tools perform it,
and does the resulting system actually satisfy the requested postconditions?
```

The system is therefore a **closed-loop engineering change orchestrator**, not another source-intelligence database.

Canonical relationship:

```text
                    SyntaxMesh
                        |
                        | ChangeAnalysisBundle
                        | VerificationPredicates
                        | evidence / contracts / impact
                        v
              SyntaxMesh Change Engine
                        |
         +--------------+---------------+
         |              |               |
         v              v               v
      planner      transform layer   verifier loop
         |              |               |
         +--------------+---------------+
                        |
                        v
              isolated changed workspace
                        |
                        v
                    SyntaxMesh
                  re-index / reason
                        |
                        v
             predicates pass / fail
```

The most important boundary is:

> **SyntaxMesh understands engineering truth. The Change Engine acts on an explicit change intent and proves the resulting change against SyntaxMesh.**

---

# 1. Non-negotiable invariants

- [ ] SyntaxMesh remains the canonical intelligence/reasoning substrate.
- [ ] The Change Engine MUST NOT maintain a competing source/ontology graph.
- [ ] Every plan step MUST be traceable to intent, evidence, a requirement, or a dependency introduced by another plan step.
- [ ] An LLM MAY propose steps or implementation tactics, but MAY NOT silently invent dependencies or claim verification without evidence.
- [ ] Transformations occur in an isolated workspace/worktree by default.
- [ ] The base repository state MUST remain recoverable.
- [ ] Every mutation is recorded as a typed effect.
- [ ] Verification is mandatory before a plan can become `VERIFIED`.
- [ ] Build/test success is not equivalent to semantic correctness.
- [ ] Semantic verification is not equivalent to runtime production correctness.
- [ ] Unknown/inconclusive evidence MUST remain explicit.
- [ ] A failed verification SHOULD drive bounded repair/replanning, not silent acceptance.
- [ ] Arbitrary shell execution MUST NOT be the primary transformation API.
- [ ] Transformation providers MUST declare capabilities and side effects.
- [ ] Repository content MUST be treated as data, not instructions to the planner/model.
- [ ] Destructive operations require explicit policy/approval according to deployment mode.
- [ ] The Change Engine MUST be useful without a hosted cloud service.
- [ ] The system MUST support fully local/self-hosted operation.
- [ ] The system SHOULD be engine/model neutral: no required LLM provider.
- [ ] Deterministic providers are preferred where they can express the change.
- [ ] Replanning must never lose the original intent or accepted constraints.

---

# 2. What belongs here vs SyntaxMesh

## SyntaxMesh owns

```text
facts
stable identities
ontology
concept mappings
derivations
truth maintenance
contracts
policies
runtime evidence
historical evidence
semantic diff
impact analysis
change requirements
scenarios
verification predicates
context packs
```

ChangeSet authoring follows the same ownership split: the Change Engine owns
the user-facing intent and grouping workflow, while SyntaxMesh owns validation,
provenance, atomic persistence, and historical queries. A prepared Change Engine
transition submits its graph and explicit `ChangeSetDelta` through
`SyntaxMeshEngine::publish_prepared_with_lineage`; consumers must not write
SyntaxMesh store tables directly. The SyntaxMesh CLI exposes read-only
ChangeSet history and does not author or assign membership.

## Change Engine owns

```text
change intent lifecycle
executable plan construction
step ordering / dependency DAG
provider selection
workspace mutation
patch application
AST/schema/config transformations
build/test command orchestration
checkpointing
approval gates
bounded repair/replanning
verification loop
result packaging
optional Git commit / PR preparation
```

## External systems may own

```text
compiler/build tool
OpenRewrite-like semantic transforms
rustfix/cargo fix
formatters/linters
schema migration generators
code generators
CI execution
Git hosting
production deployment
APM/runtime telemetry
```

The Change Engine coordinates them through explicit provider interfaces.

---

# 3. Primary workflow

A full change lifecycle:

```text
1. RECEIVE CHANGE INTENT
       |
2. RESOLVE BASE STATE
       |
3. ASK SYNTAXMESH FOR CHANGE ANALYSIS
       |
4. BUILD REQUIREMENT / CONSTRAINT SET
       |
5. GENERATE CANDIDATE PLAN DAG
       |
6. STATICALLY VALIDATE PLAN
       |
7. CREATE ISOLATED WORKSPACE
       |
8. EXECUTE TRANSFORMATIONS IN PHASES
       |
9. FORMAT / GENERATE / BUILD / TEST AS REQUIRED
       |
10. RE-INDEX THROUGH SYNTAXMESH
       |
11. EVALUATE VERIFICATION PREDICATES
       |
12a. PASS -> package verified result
12b. FAIL -> diagnose -> repair/replan -> bounded retry
12c. UNKNOWN -> require evidence/approval/policy decision
```

The key feature is the loop:

```text
transform
   -> observe new system
   -> reason through SyntaxMesh
   -> verify
   -> repair if necessary
```

This prevents a planner from assuming its own patch achieved the intended semantic result.

---

# 4. Change intent model

A change begins with an explicit intent, not directly with source edits.

Conceptual model:

```rust
pub struct ChangeIntent {
    pub id: ChangeIntentId,
    pub title: String,
    pub description: String,
    pub base: BaseRevision,
    pub targets: Vec<TargetSelector>,
    pub desired_outcomes: Vec<DesiredOutcome>,
    pub forbidden_outcomes: Vec<ForbiddenOutcome>,
    pub constraints: Vec<IntentConstraint>,
    pub scope_policy: ScopePolicy,
    pub verification_policy: VerificationPolicy,
    pub execution_policy: ExecutionPolicy,
}
```

Intent examples:

```text
Rename CustomerId semantics to GlobalCustomerId across all public representations.

Replace direct Ledger writes from Gateway with AuthorizationBoundary-mediated writes.

Migrate API v1 consumers to API v2 while keeping v1 backward-compatible until all known consumers move.

Split PaymentService ownership of refund authority into RefundCoordinator.

Remove deprecated configuration key only after all producers/consumers stop using it.
```

## 4.1 Intent must distinguish outcomes from tactics

Bad intent:

```text
Edit these seven files exactly like this.
```

Acceptable as a low-level patch request, but it provides little room for reasoning.

Better intent:

```text
All CustomerIdentity representations should use the new opaque identifier format,
while preserving v1 API compatibility and leaving persistence migration reversible.
```

The planner can derive tactics from the desired state.

## 4.2 User-supplied steps remain constraints/suggestions

If the user provides an explicit sequence, preserve it as:

```text
USER_REQUIRED_STEP
USER_SUGGESTED_STEP
```

Do not silently reorder a required step unless policy allows and the user can see the change.

---

# 5. SyntaxMesh input contract

The Change Engine should consume a stable `ChangeAnalysisBundle` from SyntaxMesh.

Conceptual structure:

```text
ChangeAnalysisBundle
    intent-normalized target
    base semantic generation
    affected entities
    affected concepts
    representations
    dependency paths
    behavior/flow impacts
    data lineage impacts
    contracts
    policies
    compatibility obligations
    build/deploy relationships
    test evidence
    runtime criticality
    historical incidents/decisions
    contradictions
    unknowns
    change requirements
    verification predicates
```

The Change Engine MUST record the exact SyntaxMesh semantic generation used to build a plan.

If the repository changes materially before execution, the plan becomes stale and must be revalidated or regenerated.

---

# 6. Plan model

Use a typed DAG, not a prose checklist.

```rust
pub struct ChangePlan {
    pub id: ChangePlanId,
    pub intent_id: ChangeIntentId,
    pub base_semantic_generation: SemanticGenerationId,
    pub steps: Vec<PlanStep>,
    pub dependencies: Vec<StepDependency>,
    pub gates: Vec<PlanGate>,
    pub final_verification: Vec<VerificationPredicateRef>,
}
```

Each step contains:

```text
step ID
step kind
purpose
input requirements
targets
provider capability
preconditions
expected effects
postconditions
dependent steps
rollback/repair metadata
evidence references
risk classification
approval requirement
```

## 6.1 Plan step kinds

Potential core step classes:

```text
SOURCE_TRANSFORM
SYMBOL_RENAME
API_SCHEMA_TRANSFORM
DATA_SCHEMA_TRANSFORM
CONFIG_TRANSFORM
DEPENDENCY_UPDATE
GENERATED_CODE_REFRESH
CODEGEN
FORMAT
LINT_FIX
BUILD
TEST
STATIC_ANALYSIS
SYNTAXMESH_REINDEX
SYNTAXMESH_VERIFY
MANUAL_ACTION
APPROVAL_GATE
GIT_STAGE
GIT_COMMIT
PATCH_EXPORT
```

Avoid a generic `RUN_COMMAND` as the normal representation.

A constrained command provider may exist for tools that cannot be modeled more specifically.

---

# 7. Plan DAG and ordering

Many engineering changes are not linear.

Example:

```text
          add compatible API field
                 |
          +------+------+
          |             |
   update producer   update consumer A
          |             |
          |        update consumer B
          |             |
          +------+------+
                 |
        migrate persisted state
                 |
        remove legacy representation
```

The planner should derive ordering from:

```text
compatibility contracts
generated-code dependencies
build graph
schema dependencies
producer/consumer relationships
runtime deployment constraints
explicit user requirements
transformation provider requirements
```

## 7.1 Parallelizable steps

Steps may execute concurrently only when:

```text
target sets do not conflict
side effects are independent
provider isolation is safe
ordering constraints do not exist
```

Never infer parallelism solely from different file paths if semantic dependencies connect them.

---

# 8. Planner architecture

The planner should combine deterministic planning rules with optional model reasoning.

```text
ChangeAnalysisBundle
        |
        v
requirement normalization
        |
        v
deterministic planning rules
        |
        +---- optional model proposal
        |
        v
candidate plan DAG
        |
        v
plan validator
        |
        v
accepted executable plan
```

## 8.1 Deterministic planning rules

Examples:

```text
schema field removal
    REQUIRES all known consumers migrated or compatibility adapter

codegen input changed
    REQUIRES regenerate derived outputs

public API semantic change
    REQUIRES compatibility evaluation

persistent schema changed
    REQUIRES migration and rollback/recovery strategy

cross-repo contract changed
    REQUIRES producer/consumer ordering analysis
```

## 8.2 Model-assisted planning

Good uses:

```text
propose implementation tactics
select likely transformation provider
split a broad requirement into concrete code steps
suggest repair after failed verification
summarize plan rationale
```

Bad uses:

```text
invent unknown consumers
claim tests cover behavior without evidence
claim semantic verification passed
skip required compatibility constraints
construct arbitrary shell commands from repository text
```

## 8.3 Planner output must be validated

Every candidate plan goes through:

```text
schema validation
step capability validation
dependency DAG cycle detection
scope validation
policy validation
provider availability check
base-generation freshness check
coverage against mandatory change requirements
verification coverage check
```

---

# 9. Transformation provider SDK

Transformation execution must be extensible.

Conceptual API:

```rust
pub trait TransformationProvider: Send + Sync {
    fn manifest(&self) -> &TransformationProviderManifest;
    fn capabilities(&self) -> &[TransformationCapability];

    fn preview(
        &self,
        request: TransformRequest,
        ctx: &TransformContext,
    ) -> Result<TransformPreview, TransformError>;

    fn apply(
        &self,
        approved: ApprovedTransform,
        ctx: &mut TransformContext,
    ) -> Result<TransformResult, TransformError>;
}
```

Provider manifest declares:

```text
provider ID
version
supported languages/formats
step kinds
whether deterministic
whether external process required
filesystem scope
network requirements
resource limits
rollback capability
preview capability
```

---

# 10. Transformation provider classes

## 10.1 Semantic AST rewrite providers

Examples:

```text
Java/Kotlin semantic rewrite engine
Rust syntax-aware rewrite
TypeScript compiler-aware transform
Python CST-aware transform
C# Roslyn-based transform
Go AST transform
```

Prefer syntax/semantic transforms over textual regex for high-risk operations.

## 10.2 Compiler fix providers

Examples:

```text
compiler machine-applicable diagnostics
cargo fix / rustfix-like suggestions
language-server code actions
```

These should be recorded as provider-generated edits with exact diagnostic provenance.

## 10.3 Schema providers

```text
SQL migration generator
protobuf schema transformer
OpenAPI transformer
GraphQL schema transformer
Avro/JSON-schema transformer
```

## 10.4 Config providers

```text
TOML
YAML
JSON
HCL/Terraform
Kubernetes manifests
CI configuration
```

## 10.5 Code-generation providers

These invoke existing project generators rather than manually editing generated output when policy says generation is canonical.

## 10.6 Text patch provider

A generic patch provider remains useful for:

```text
documentation
comments
simple files
unsupported languages
small deterministic replacements
```

But it should be lower preference for structural source changes.

---

# 11. Transformation selection

The planner should choose the strongest safe provider available.

Illustrative order:

```text
project-native codemod
compiler-provided fix
semantic AST/CST transform
schema-aware transformer
syntax-aware transformer
bounded text patch
manual action
```

This is not universal; capabilities and intent determine selection.

Every selection should explain:

```text
why provider chosen
what alternatives existed
whether the transformation is deterministic
known limitations
```

---

# 12. Isolated workspace model

Never mutate the user's canonical working tree by default.

Preferred execution targets:

```text
Git worktree
isolated temporary clone
snapshot-backed workspace
containerized workspace where needed
```

Record:

```text
base commit
working-tree hash
workspace ID
branch/worktree ID
uncommitted baseline changes
```

## 12.1 Dirty working trees

If a change intentionally starts from uncommitted work:

```text
capture base tree state
capture dirty diff
bind plan to that exact state
```

Never accidentally drop or overwrite unrelated user edits.

---

# 13. Preview-before-apply

Every transform provider SHOULD support a preview where practical.

Preview may contain:

```text
files affected
symbols affected
structured edits
diff
provider diagnostics
expected semantic effects
known uncertainties
```

The planner can compare previewed effects with plan expectations before mutating the workspace.

For high-risk steps, policy may require explicit approval after preview.

---

# 14. Effect journal

Every external side effect should be journaled.

```text
step started
provider selected
preview hash
approval obtained
files before hashes
files after hashes
command/tool version
stdout/stderr digest where relevant
result classification
rollback artifact
```

This is essential for recovery and auditability.

Do not rely solely on Git diff as workflow state.

---

# 15. Penelope integration — strongly recommended internal workflow engine

The Change Engine is an almost textbook durable workflow system.

Use Penelope for:

```text
plan execution
step retries
provider timeouts
stale result rejection
approval waits
checkpointing
repair loops
reconciliation after process crash
multi-repository coordination
rollback/compensation where meaningful
```

Potential saga:

```text
ExecuteChangePlanSaga
    -> prepare workspace
    -> execute phase A
    -> verify phase A
    -> execute phase B
    -> build/test
    -> SyntaxMesh reindex
    -> semantic verify
    -> package result
```

Penelope should not sit inside individual AST visitor operations.

---

# 16. Stale-plan protection

A plan is bound to:

```text
base repository revisions
base SyntaxMesh semantic generation
provider versions
relevant config/toolchain fingerprints
```

Before each major phase, validate freshness.

Possible classifications:

```text
FRESH
SAFE_TO_REBASE
REQUIRES_REANALYSIS
INVALIDATED
```

Example:

```text
another change modified unrelated docs
    -> likely FRESH/SAFE

another change modified CustomerId schema
    -> REQUIRES_REANALYSIS
```

SyntaxMesh should help determine whether the changed region intersects plan assumptions.

---

# 17. Incremental planning

Do not regenerate the entire plan after every small verification failure.

Track dependencies:

```text
requirement
   -> plan steps
   -> effects
   -> verification predicates
```

If one predicate fails, invalidate only affected steps and downstream plan nodes where possible.

---

# 18. Verification architecture

Verification must be multi-layered.

```text
LAYER 1: syntactic
LAYER 2: compile/type/build
LAYER 3: tests
LAYER 4: static/program analysis
LAYER 5: SyntaxMesh structural verification
LAYER 6: SyntaxMesh semantic/contract verification
LAYER 7: optional runtime/sandbox verification
```

Passing a lower layer does not imply passing a higher layer.

---

# 19. Syntax verification

Examples:

```text
files parse
format valid
schema syntax valid
no malformed generated output
```

Cheap; execute early.

---

# 20. Build/type verification

Use project-native tools.

Examples:

```text
cargo check
cargo test --no-run
tsc
mvn/gradle compile
go test/build
.NET build
```

Represent as typed verification tasks, not opaque command strings where possible.

---

# 21. Test verification

SyntaxMesh can help select affected tests, but the Change Engine executes them through test providers.

Support:

```text
targeted tests
package/module tests
integration tests
full suite
property tests
fuzz targets
scenario tests
```

Record exact test selection rationale.

If SyntaxMesh says relevant behavior lacks test coverage, the plan should surface that gap rather than pretending verification is complete.

---

# 22. Static/program-analysis verification

Re-run relevant providers after changes:

```text
data-flow analysis
taint analysis
lints
security scans
API compatibility
schema compatibility
```

Compare before/after findings semantically rather than only by raw tool output.

---

# 23. SyntaxMesh structural verification

After transformation:

```text
re-index changed workspace
obtain new graph generation
compare expected structural effects
```

Examples:

```text
old symbol removed
new symbol exists
all expected references resolved
forbidden dependency removed
new dependency path created only where intended
```

---

# 24. SyntaxMesh semantic verification

This is the defining verification layer.

Evaluate the predicates generated from the original intent.

Examples:

```text
CustomerIdentity representations all map to V2 encoding
API v1 remains compatible
Gateway no longer has a path to Ledger bypassing AuthorizationBoundary
refund authority now belongs to RefundCoordinator
no new consumer references deprecated API
PII does not cross prohibited region boundary
purchase flow still satisfies required compensation contract
```

A plan is not `VERIFIED` until mandatory semantic predicates pass or policy explicitly accepts unresolved predicates.

---

# 25. Verification result model

```text
VerificationResult {
    predicate,
    status,
    evidence,
    graph_generation,
    semantic_generation,
    tool/provider results,
    diagnostics,
}
```

Statuses:

```text
PASS
FAIL
UNKNOWN
INCONCLUSIVE
NOT_APPLICABLE
WAIVED
```

Avoid boolean-only verification.

---

# 26. Repair and replan loop

When verification fails:

```text
failed predicate
      |
      v
SyntaxMesh explanation / counterexample
      |
      v
classify failure
      |
  +---+------------------+
  |                      |
  v                      v
local repair         plan invalid
  |                      |
  v                      v
new step             partial replan
  +----------+-----------+
             |
             v
          reapply
             |
             v
          reverify
```

Bound the loop:

```text
max repair attempts
max changed scope expansion
max token/model budget
max wall-clock/resource budget
```

After bounds are exceeded, return explicit unresolved state rather than thrashing.

---

# 27. Failure classification

Potential classes:

```text
TRANSFORM_FAILED
BUILD_FAILED
TEST_FAILED
STATIC_ANALYSIS_FAILED
SEMANTIC_PREDICATE_FAILED
PLAN_ASSUMPTION_INVALID
BASE_STATE_CHANGED
PROVIDER_UNAVAILABLE
AMBIGUOUS_INTENT
INSUFFICIENT_EVIDENCE
POLICY_BLOCKED
APPROVAL_REQUIRED
```

Each class drives a different recovery strategy.

---

# 28. Rollback and compensation

Source edits are usually easy to rollback through workspace snapshots/Git.

External side effects are harder.

Default v1 should avoid production side effects.

For supported reversible actions, providers may expose:

```text
rollback token
inverse patch
restore snapshot
migration-down operation
```

Never claim rollback is safe when it is not.

---

# 29. Multi-repository changes

SyntaxMesh can identify cross-repository contracts; the Change Engine should plan coordinated changes across repositories.

Example:

```text
schema repo
backend producer
SDK repo
frontend consumer
infra repo
```

Plan must include:

```text
repo-specific base revisions
cross-repo dependencies
compatibility windows
publish ordering
verification across all updated graphs
```

## 29.1 No fake global transaction

A multi-repository change is not atomic.

Use staged compatibility and explicit failure recovery.

---

# 30. Compatibility-first migrations

Prefer expand/migrate/contract patterns.

Example:

```text
1. add new representation while retaining old
2. upgrade consumers
3. upgrade producers
4. migrate stored state
5. verify no remaining old consumers
6. remove old representation
```

SyntaxMesh provides evidence about consumers and contracts.

The Change Engine converts that into execution sequencing.

---

# 31. Database/schema migrations

Do not casually generate destructive SQL.

Schema transformation providers should classify:

```text
additive
compatible
requires backfill
requires dual-write
requires read compatibility
requires downtime
potentially destructive
irreversible
```

Plan verification should include:

```text
schema compatibility
application references
migration code presence
rollback/recovery strategy
```

Actual production migration execution should remain outside the default repository-change mode.

---

# 32. API and event migrations

First-class concepts:

```text
producer
consumer
schema version
compatibility mode
migration window
deprecation
```

Use SyntaxMesh to identify known consumers across repositories and runtime evidence.

Unknown external consumers remain an explicit risk.

---

# 33. Generated code

Generated files require special handling.

If source-of-truth inputs are known:

```text
modify generator input
run codegen
verify outputs
```

Do not edit generated output directly unless policy explicitly allows it.

SyntaxMesh should expose generated-from lineage where available.

---

# 34. Formatting and non-semantic churn

Apply formatters after structural transformations where appropriate.

Keep semantic diff separate from formatting churn.

A successful plan should be able to report:

```text
semantic changes
mechanical generated changes
format-only changes
```

---

# 35. Git integration

Git is a packaging/version surface, not workflow state.

Potential optional outputs:

```text
patch file
working tree
branch
one commit
multiple commits by plan phase
commit message proposal
PR description proposal
```

## 35.1 Commit structure

Plan phases may map to commits where useful:

```text
compatibility scaffolding
consumer migration
producer migration
cleanup
```

But do not force commit-per-step.

---

# 36. Pull request preparation

Optional provider may create a PR only with explicit user policy/authorization.

Generated PR description can include:

```text
intent
semantic changes
affected concepts
contracts checked
verification results
known unknowns
migration sequence
risk notes
```

The Change Engine must never claim review approval.

---

# 37. Human approval gates

Support configurable gates:

```text
before any mutation
before high-risk transform
before destructive schema operation
before scope expansion
before Git commit
before PR creation
```

Approval should bind to the exact plan/preview hash so later plan mutation invalidates stale approval.

---

# 38. Autonomy profiles

Possible profiles:

```text
ADVISORY
    plan only, no mutation

PREVIEW
    generate previews/diffs, no persistent mutation

LOCAL_AUTONOMOUS
    mutate isolated worktree, build/test/verify, do not publish

PR_READY
    prepare commits/branch/PR artifact; publication requires policy
```

Do not jump directly to production-deployment autonomy.

---

# 39. Security model

The Change Engine has a much larger attack surface than SyntaxMesh because it mutates files and may execute tools.

Threats:

```text
prompt injection in repository text
malicious build scripts
malicious compiler/plugin
path traversal
symlink attacks
command injection
credential leakage
network exfiltration
unbounded process spawning
fork bombs
resource exhaustion
supply-chain tool substitution
malicious generated patches
```

Required controls:

```text
workspace sandbox
filesystem allowlist
process capability model
network denied by default where practical
environment-variable allowlist
secret redaction
resource limits
process timeout
output limits
symlink/path validation
provider signing/checksums where appropriate
tool-version pinning
explicit trust classification
```

---

# 40. Command execution boundary

Some ecosystems require commands.

Represent commands as constrained tool invocations:

```text
ToolInvocation {
    tool_id,
    argv,
    cwd_scope,
    env_profile,
    network_policy,
    timeout,
    resource_budget,
}
```

Do not use shell interpolation by default.

Prefer direct argv execution.

---

# 41. Provider trust levels

Potential levels:

```text
BUILT_IN
TRUSTED_LOCAL
SIGNED_THIRD_PARTY
UNTRUSTED_SANDBOXED
```

A provider's trust level controls available capabilities.

An untrusted provider must not gain arbitrary process/network/filesystem access merely because it can generate a patch.

---

# 42. Model provider boundary

LLMs should sit behind a planner/reasoning interface.

```rust
pub trait PlanningModel {
    fn propose_plan(...);
    fn propose_repair(...);
    fn explain_plan(...);
}
```

Model outputs are untrusted candidate structures until validated.

Record:

```text
provider
model
revision
prompt/template fingerprint
context hashes
response hash
```

No model lock-in.

---

# 43. Agent interoperability

The Change Engine itself may be used by an external coding agent.

Surfaces:

```text
CLI
MCP
HTTP/API
Rust library
```

Potential high-level MCP tools:

```text
analyze_change
plan_change
preview_plan
execute_plan
plan_status
verification_status
explain_failure
export_patch
```

Avoid exposing low-level unrestricted process execution over MCP.

---

# 44. SyntaxMesh context use

The planner should request context from SyntaxMesh based on plan needs rather than dumping the repository into an LLM.

Examples:

```text
requirements context
concept representations
contract evidence
consumer lists
migration history
relevant tests
incident history
```

This preserves the token-efficiency advantage of SyntaxMesh.

---

# 45. Evidence density over context volume

Planner/model context should optimize for:

```text
mandatory constraints
exact dependency evidence
semantic uniqueness
unknowns
counterexamples
historical failures
```

Not maximum token consumption.

---

# 46. Dry-run mode

A plan should be runnable in dry-run mode:

```text
resolve providers
validate capabilities
preview transformations
estimate affected files/entities
simulate plan DAG
run counterfactual SyntaxMesh scenario where possible
```

No persistent source mutation.

This is valuable for high-risk changes and human review.

---

# 47. Counterfactual-first planning

Before expensive transformations, use SyntaxMesh scenarios/counterfactual overlays to validate the target architecture where possible.

Example:

```text
intent: move refund authority

SyntaxMesh scenario:
PaymentService authority removed
RefundCoordinator authority added

check:
contracts
cycles
dependencies
data paths
ownership
```

If the target state is already semantically invalid, do not waste time implementing it.

---

# 48. Plan completeness

A plan is complete only if every mandatory `ChangeRequirement` is either:

```text
SATISFIED_BY_STEP
ALREADY_SATISFIED
WAIVED
MANUAL
UNKNOWN_BLOCKING
```

Do not allow requirements to disappear during planning.

---

# 49. Verification coverage

Every desired outcome should map to one or more verification predicates.

Every forbidden outcome should map to a negative predicate where possible.

Example:

```text
Desired:
all payment writes pass AuthorizationBoundary

Predicate:
NoPath(payment-entry, FinancialLedger, bypassing=AuthorizationBoundary)
```

If an intent cannot be verified automatically, mark it `MANUAL_VERIFICATION_REQUIRED`.

---

# 50. Unknowns are first-class

Example unknowns:

```text
external consumer cannot be observed
runtime telemetry unavailable
schema registry incomplete
dynamic reflection prevents static resolution
production-only behavior not reproducible locally
```

Planner behavior may be:

```text
block
require approval
add compatibility cushion
require manual step
continue with warning
```

according to policy.

---

# 51. Risk model

Risk should be evidence-derived and decomposed rather than one opaque score.

Dimensions:

```text
semantic scope
runtime criticality
persistence impact
external compatibility
security boundary impact
number of repositories
unknown consumer exposure
rollback difficulty
test coverage gap
incident history
provider uncertainty
```

The Change Engine may summarize them but should retain components.

---

# 52. Plan explanations

Every step should answer:

```text
Why is this step necessary?
Which requirement does it satisfy?
Which evidence supports it?
What depends on it?
What happens if it is omitted?
How will we verify it?
```

This should be machine-derived where possible.

---

# 53. Repair explanations

When repair is proposed:

```text
Predicate P failed because path A -> B -> C still exists.
Step S changed B but alternate path D remains.
Repair R targets D because it is the remaining counterexample.
```

This is much more trustworthy than “the model decided to try another patch.”

---

# 54. Transformation provenance

Every changed line/file should be attributable to:

```text
plan step
provider
provider version
intent
requirement
model suggestion if applicable
manual edit if applicable
```

This is especially useful for generated agentic changes.

---

# 55. Manual edits during execution

Humans may edit the isolated worktree mid-plan.

When detected:

```text
hash changed outside known provider effect
```

record `MANUAL_MUTATION` and re-run impact/freshness analysis.

Do not silently attribute manual edits to the automated plan.

---

# 56. Resume after crash

Persist:

```text
intent
accepted plan
workspace identity
completed steps
provider result hashes
approvals
verification state
repair attempts
```

On restart:

```text
verify workspace state
reconcile uncertain effects
resume safe step
```

This is a major Penelope use case.

---

# 57. Idempotency

All plan/step IDs must be stable.

Provider applications should accept an idempotency key where meaningful.

Never double-apply a migration or generator because a response was lost.

---

# 58. Generated artifacts and Shardline integration

Shardline is a good optional store for immutable large artifacts:

```text
workspace snapshot
patch bundle
plan bundle
build logs
verification artifacts
large generated outputs
benchmark/replay artifacts
```

Do not use Shardline as mutable workflow state.

---

# 59. StateChronicle integration

Optional high-assurance mode may publish verified plan/result manifests.

Useful for:

```text
regulated change workflows
security-sensitive refactors
reproducible CI evidence
signed automated migration records
```

Record plan/result generation boundaries, not every file write.

SyntaxMesh and Change Engine must work without this mode.

---

# 60. TrustGrant integration

In multi-user/enterprise deployments, TrustGrant may be useful for delegated authority such as:

```text
who may approve destructive transforms
who may create PRs
who may use network-enabled providers
who may execute specific high-risk provider classes
```

Do not turn TrustGrant into ordinary login/session management.

---

# 61. Build-system integration

Build providers should understand workspace topology where possible.

Examples:

```text
Cargo workspace
Bazel targets
npm/pnpm workspace
Gradle/Maven modules
Go modules
.NET solution
```

SyntaxMesh can supply affected build targets; the Change Engine executes minimal relevant verification first, then expands as policy requires.

---

# 62. Test selection strategy

Progressive verification:

```text
1. directly affected unit tests
2. affected package tests
3. impacted integration tests
4. contract/API compatibility tests
5. broader suite according to risk
```

Never skip mandatory suites merely because targeted tests pass.

---

# 63. Refactoring providers vs generated patches

Prefer deterministic refactoring providers for mechanical changes.

Use LLM-generated patches where semantic implementation work is genuinely needed.

Example:

```text
rename symbol -> compiler/AST rename provider
update protobuf field -> schema provider
rewrite business logic -> coding model patch
```

This reduces unnecessary model risk and token cost.

---

# 64. Coding-model patch provider

For changes requiring code synthesis, define a patch provider that receives bounded SyntaxMesh context plus explicit requirements.

Input:

```text
step purpose
allowed files/symbols
relevant code excerpts
contracts
postconditions
forbidden effects
historical warnings
```

Output:

```text
structured patch
claimed requirement coverage
uncertainties
```

Claims remain untrusted until re-index/verification.

---

# 65. Scope expansion

If a provider discovers that additional files/entities must change:

```text
requested scope
    -> proposed scope expansion
```

The engine must:

```text
ask SyntaxMesh for impact
update plan requirements
apply policy/approval
then expand
```

Do not let an LLM silently spread edits across the repository.

---

# 66. No-op detection

Before applying a plan, check whether the desired outcome already holds.

Possible result:

```text
NO_CHANGE_REQUIRED
```

Do not create churn for already-satisfied intents.

---

# 67. Semantic minimality

Prefer the smallest change that satisfies the intent and predicates while respecting architecture constraints.

Do not equate smallest diff with smallest semantic change.

Sometimes a larger compatibility scaffold is safer than a tiny breaking edit.

---

# 68. Semantic diff after execution

Always produce an after/before semantic diff through SyntaxMesh.

Report separately:

```text
intended semantic changes
incidental semantic changes
unexpected semantic changes
purely mechanical changes
```

Unexpected semantic changes should fail or require approval according to policy.

---

# 69. Regression guard generation

When appropriate, the planner may propose adding a test or policy that makes the corrected invariant durable.

Example:

```text
incident-driven fix
    -> code change
    -> contract predicate
    -> regression test
```

Do not automatically add meaningless tests merely to increase coverage.

---

# 70. Historical precedent

SyntaxMesh can surface previous similar migrations.

The planner may reuse proven patterns:

```text
same API version transition
same schema expansion pattern
same service split
same ownership migration
```

Historical precedent is evidence, not a guarantee that the current case is identical.

---

# 71. Planning templates

Support reusable typed migration patterns:

```text
RenamePublicConcept
MigrateApiVersion
ReplaceDependency
SplitServiceResponsibility
MoveAuthority
DeprecateConfigKey
SchemaExpandMigrateContract
IntroduceAuthorizationBoundary
MoveDataRegion
```

Templates provide deterministic skeletons that the planner specializes using SyntaxMesh evidence.

---

# 72. Policy profiles

Organizations may define:

```text
low-risk refactor policy
public API migration policy
security-boundary policy
persistence migration policy
cross-repo release policy
regulated change policy
```

Profiles decide:

```text
required verification layers
approval gates
allowed providers
network access
minimum tests
rollback requirements
```

---

# 73. Verification budgets

Verification can be expensive.

Support policies such as:

```text
fast local
standard PR
high assurance
release candidate
```

But never silently downgrade mandatory predicates because of budget.

---

# 74. Caching

Cache deterministic provider results by:

```text
input hashes
provider/version
config/toolchain
step parameters
```

Do not cache successful verification across a changed semantic generation unless inputs remain equivalent.

---

# 75. Performance architecture

Separate latency classes:

```text
HOT
plan status
small previews
local patch application

WARM
SyntaxMesh re-index
build/check
targeted tests

COLD
full suite
multi-repo analysis
large semantic migration
historical replay
```

Do not make UI/CLI status depend on cold work completing synchronously.

---

# 76. Concurrency

Safe parallelism exists in:

```text
independent repositories
independent generated-code refreshes
non-overlapping test suites
read-only verification
```

Mutation of overlapping file sets should serialize unless providers explicitly support merging.

---

# 77. Resource limits

Bound:

```text
concurrent provider processes
CPU
memory
disk growth
workspace count
patch size
files changed
network requests
model tokens
repair loops
```

Large scope requires explicit policy.

---

# 78. Observability

Expose:

```text
plan phase
current step
provider latency
files changed
verification progress
replan count
failure class
```

Do not leak source/secrets into telemetry by default.

---

# 79. Audit trail

A completed plan can export:

```text
intent
base revisions
SyntaxMesh semantic generation
plan DAG
approvals
provider versions
patch hashes
build/test results
semantic diff
verification predicates/results
unknowns/waivers
final commit/patch identities
```

This should be deterministic enough for review and archival.

---

# 80. Result model

Possible terminal states:

```text
PLANNED
PREVIEWED
APPLIED_UNVERIFIED
VERIFIED
VERIFIED_WITH_WAIVERS
NO_CHANGE_REQUIRED
FAILED
BLOCKED
CANCELLED
STALE
MANUAL_INTERVENTION_REQUIRED
```

Do not use `SUCCESS` for ambiguous partially verified outcomes.

---

# 81. Output artifacts

Potential outputs:

```text
change-plan.json / typed binary equivalent
human-readable plan.md
patch.diff
semantic-diff.ndjson
verification-report.json
verification-report.md
workspace snapshot reference
Git branch/commit references
```

Public formats must be versioned.

---

# 82. CLI concept

Possible commands:

```text
sm-change analyze <intent>
sm-change plan <intent>
sm-change preview <plan>
sm-change apply <plan>
sm-change verify <workspace>
sm-change explain <plan-or-step>
sm-change status <run>
sm-change export <run>
```

Naming is illustrative.

---

# 83. API model

Keep transport-neutral application services:

```text
AnalyzeChange
CreatePlan
ValidatePlan
PreviewPlan
ExecutePlan
ResumeRun
VerifyRun
ExplainStep
ExportRun
```

CLI/MCP/HTTP call the same application operations.

---

# 84. Storage

Persistent state includes:

```text
intents
plans
plan versions
runs
steps
approvals
provider effects
verification results
repair attempts
artifact references
```

Use a storage trait.

Do not store canonical SyntaxMesh graph state here.

---

# 85. Plan versioning

A plan changes during repair/replanning.

Use immutable versions:

```text
Plan v1
  -> verification failure
Plan v2
  -> scope expansion
Plan v3
```

Preserve lineage and reasons.

---

# 86. Plan identity

Plan identity should bind:

```text
intent hash
base semantic generation
requirement set hash
provider capability set
planner configuration
```

A changed intent creates a new plan lineage or explicit revision.

---

# 87. Deterministic planning where possible

Migration templates and deterministic provider selection should produce reproducible plan skeletons.

LLM influence must be recorded separately so identical inputs are not falsely claimed to yield deterministic plans.

---

# 88. Replay

A historical run should be replayable in a sandbox where tool versions/artifacts are available.

Useful for:

```text
regression testing
planner upgrades
provider upgrades
model comparison
incident analysis
```

---

# 89. Benchmark families

## 89.1 Planning quality

Measure:

```text
mandatory requirements covered
unnecessary steps
missing dependencies
ordering correctness
scope precision
unknown detection
```

## 89.2 Transformation quality

```text
patch correctness
minimal unintended churn
provider determinism
compile rate
test pass rate
```

## 89.3 Verification quality

```text
true regression detection
false positive rate
semantic predicate correctness
unexpected semantic change detection
```

## 89.4 Agent efficiency

```text
tokens
turns
tool calls
repair attempts
wall-clock
files manually inspected
```

Compare:

```text
coding agent alone
coding agent + SyntaxMesh
coding agent + SyntaxMesh Change Engine
```

---

# 90. Ground-truth benchmark tasks

Include:

```text
symbol/API rename
cross-repo type migration
schema expansion/contract migration
authorization-boundary insertion
service responsibility split
deprecated API removal
config-key migration
data-residency correction
incident regression fix
```

Each benchmark should have known acceptance predicates.

---

# 91. What NOT to build initially

- [ ] production deployment orchestrator
- [ ] generic CI platform
- [ ] IDE frontend
- [ ] custom programming-language compiler
- [ ] universal AST rewrite engine from scratch
- [ ] package manager
- [ ] hosted Git forge
- [ ] arbitrary shell agent
- [ ] always-on cloud requirement
- [ ] model-provider lock-in
- [ ] opaque autonomous changes with no evidence trail
- [ ] automatic production DB migrations
- [ ] automatic merge-to-main

Use existing specialized tools through adapters.

---

# 92. v0.1 definition

A coherent first version could support:

```text
one repository/worktree
SyntaxMesh ChangeAnalysisBundle input
explicit ChangeIntent
plan DAG
manual + model-assisted planner
text patch provider
Rust syntax-aware provider or one other high-quality language provider
format/build/test providers
isolated worktree execution
Penelope-backed durable run
SyntaxMesh re-index after change
verification predicates
repair loop with strict bound
patch/report export
```

Do not block v0.1 on ten languages or cross-repo migrations.

---

# 93. v0.2 / v0.x expansion

```text
additional semantic transform providers
schema/config providers
compiler fix providers
multi-repo planning
approval gates
Git commit packaging
historical precedent
planning templates
higher-assurance sandboxing
```

---

# 94. v1 bar

Potential 1.0 requirements:

```text
stable plan/intent/provider APIs
stable SyntaxMesh change-analysis contract
stable verification predicate model
multiple high-quality language transformation providers
multi-repository changes
schema/API migration support
robust crash recovery
strong sandbox/process policy
human approval model
reproducible plan/version history
semantic diff verification
bounded repair/replanning
Git/PR packaging adapters
large-repo validation
security threat model
benchmark corpus
```

---

# 95. First implementation order

```text
1. core intent/plan/result types
2. SyntaxMesh client + ChangeAnalysisBundle contract
3. storage trait
4. Penelope run orchestration
5. isolated Git worktree manager
6. provider manifest/capability SDK
7. text patch provider
8. build/test tool providers
9. SyntaxMesh re-index + predicate verification
10. CLI
11. model-assisted planner interface
12. first semantic source transformer
13. repair/replan loop
14. audit/export artifacts
15. approval gates
16. multi-repo support
```

This validates the closed loop before expanding provider breadth.

---

# 96. Suggested Cargo workspace

```text
crates/
  change-core/
  change-api-model/
  change-syntaxmesh-client/
  change-planner/
  change-plan-validator/
  change-runner/
  change-workspace/
  change-provider-sdk/
  change-provider-text/
  change-provider-rust/
  change-provider-build/
  change-provider-test/
  change-verifier/
  change-penelope/
  change-git/
  change-export/
  change-cli/
  change-mcp/
  change-http/
  change-bench/
```

Names are illustrative.

Keep domain/planning types free of transport/runtime dependencies.

---

# 97. Relationship to coding agents

The Change Engine should not try to replace general coding agents.

A coding agent can act as one transformation provider and one planner assistant.

The stronger system is:

```text
SyntaxMesh
    = engineering truth / context / constraints

Change Engine
    = planning / mutation workflow / verification

Coding model
    = implementation reasoning and synthesis where deterministic tools are insufficient

Compiler/tests/analyzers
    = executable feedback
```

This makes models replaceable and keeps the safety/verification loop outside the model.

---

# 98. Why this should remain a separate project

Combining this into SyntaxMesh would damage both projects.

SyntaxMesh benefits from being:

```text
read-mostly
rebuildable
deterministic where possible
safe to embed
usable in CI/IDE/query paths
low side-effect
```

The Change Engine necessarily needs:

```text
filesystem mutation
process execution
worktree management
approval waits
retries
repair loops
patch generation
potential network/tool invocation
```

These have fundamentally different security, reliability, API, and operational constraints.

Separate projects keep those boundaries auditable.

---

# 99. Long-term product loop

The mature loop should become:

```text
                     user / agent intent
                            |
                            v
                       SyntaxMesh
                   understand / constrain
                            |
                            v
                  Change Analysis Bundle
                            |
                            v
                    Change Engine plan
                            |
                            v
               deterministic transforms first
                 model-generated code where needed
                            |
                            v
                   isolated changed system
                            |
             +--------------+--------------+
             |                             |
             v                             v
        compiler/tests               SyntaxMesh reindex
             |                             |
             +--------------+--------------+
                            |
                            v
                     semantic verify
                            |
               +------------+------------+
               |                         |
               v                         v
             pass                    counterexample
               |                         |
               v                         v
         verified patch            repair / replan
```

The system should never trust the plan merely because the plan generated the patch.

---

# 100. Final responsibility statement

> **SyntaxMesh Change Engine converts evidence-backed engineering intent into bounded transformations and proves the result against SyntaxMesh.**

It is responsible for:

```text
planning
ordering
provider selection
safe mutation
workflow durability
verification orchestration
repair/replanning
result packaging
```

It is not responsible for creating its own competing understanding of the software system.

The authoritative understanding comes from SyntaxMesh.

The defining invariant is:

> **Every automated change must close the loop from intent to evidence to transformation back to independently recomputed evidence.**

That closed loop is what makes the system more than an autonomous patch generator.
