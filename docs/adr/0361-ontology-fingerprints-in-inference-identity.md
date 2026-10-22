# ADR-0361: Ontology Fingerprints in Inference Identity

Status: accepted

Date: 2026-10-22

## Context

Ontology definitions are supplied to the registered inference path, but rules
and claims currently identify only ontology namespace/version strings. A caller
can replace a definition without changing those strings, leaving rule-set,
claim, and materialization identities unchanged even though type-checking
semantics have changed. Definition collection order is also not semantic and
must not perturb identity.

## Decision

1. Compute a deterministic fingerprint for each complete ontology definition.
   Canonicalization treats concept IDs, relation declarations, and each
   relation's subject/object concept constraints as sets; namespace, version,
   schema version, and the explicit instance relation are identity inputs.
2. `OntologyRegistry::bind_rules` returns an opaque `BoundRuleSet`. For each
   rule, it derives an effective configuration fingerprint from the original
   configuration fingerprint and the exact ontology definitions used by that
   rule. The bound rules are exposed read-only for independent persistence and
   witness verification; registered evaluation verifies that the set belongs
   to the registry before using it.
3. Rule-set, claim, and materialization identities consume those effective
   fingerprints. A schema change therefore cannot reuse claims or durable
   operation identities under the same human-readable ontology version.
4. Keep fingerprinting and binding in the pure inference crate. Do not add
   fields to serialized `Rule`, `Claim`, or transport DTOs: bound identity fits
   in the existing configuration fingerprint, preserving bincode journal and
   SQLite payload layouts as well as the retained inference DTO schema.

## Consequences

Applications that use registered ontology definitions can call the registry's
rule-binding method before persisting or independently verifying rules, and
must persist/verify the returned read-only bound rules rather than the original
rules. The registered Query path binds before retained-history reads and
evaluates with that same bound set. Rules bound to changed definitions have
different identities even when namespace/version labels are unchanged. This
does not persist ontology definitions or define semantic compatibility between
ontology versions; durable registry storage and migrations remain separate
work.

## Verification

Pure tests establish order-independent definition fingerprints, identity change
for schema edits at a fixed version, opaque bound-set validation, and legacy
witness verification. Existing Rule/Claim bincode and inference DTO layouts
remain unchanged. Query fixtures verify registered inference binds its output
consistently across reference and durable backends.

## References

- [ADR-0358](0358-bounded-ontology-schema-admission.md)
- [ADR-0359](0359-bounded-ontology-relation-signatures.md)
- [ADR-0360](0360-explicit-ontology-instance-membership.md)
- [SyntaxMesh v5 source of truth §78.5](../syntaxmesh-todo-extensible-opensource-v5.md)
- [V0 architecture and implementation plan](../V0_ARCHITECTURE_PLAN.md)
