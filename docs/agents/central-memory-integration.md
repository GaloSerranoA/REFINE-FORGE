# Central Memory Integration

Refine-Forge should support HELYX, COGN8TY, and Consciousness-rs memory without
making memory a source of trust. The right integration point is a compatibility
contract first, then an optional bridge to a concrete backend such as
`helyx-memory`.

Local source inspected on 2026-05-24:

- `C:\HELYX\crates\helyx-memory`
- `D:\AI-PROJECTS-GALO\PROJECTS\NANTAR INMORTAL RUST\crates\immortal-memory`
- `D:\AI-PROJECTS-GALO\PROJECTS\Consciousness-rs-parity\crates\memory`
- `C:\HELYX\Cargo.toml`
- `D:\AI-PROJECTS-GALO\PROJECTS\NANTAR INMORTAL RUST\Cargo.toml`
- `D:\AI-PROJECTS-GALO\PROJECTS\Consciousness-rs-parity\Cargo.toml`
- `D:\AI-PROJECTS-GALO\PROJECTS\Consciousness-rs-parity\docs\evidence-standard.md`
- `D:\AI-PROJECTS-GALO\PROJECTS\Consciousness-rs-parity\docs\refineforge`
- `D:\AI-PROJECTS-GALO\PROJECTS\Refine-Forge\Cargo.toml`

## Finding

`helyx-memory` and `immortal-memory` are strong candidate backends. Both expose
a tiered memory store, context window, working memory, retrieval engine,
reciprocal-rank fusion, SQLite-backed storage, semantic and temporal graph
stores, compression, provenance, cross-linking, and optional vector memory.

`consciousness-memory` is also useful, but with a different role. It exposes
weighted token memory, Markov chains, persistent core memory, retrieval,
reciprocal-rank fusion, memory graphs, MemCube records, vector helpers, bloom
filters, compression, and provenance algorithms. The wider
`Consciousness-rs-parity` project also carries a local evidence standard,
claim register, Refine-Forge-style claim YAML files, Lean structural claims,
and bundle metadata.

The crate is not a drop-in open-source Refine-Forge dependency today:

- It inherits HELYX workspace metadata: edition `2024`, license `UNLICENSED`,
  and repository `https://helyx.local/not-yet-published`.
- Refine-Forge is edition `2021` and license `Apache-2.0 OR MIT`.
- It depends on HELYX workspace crates such as `helyx-crypto`,
  `helyx-foundation`, `helyx-time`, and optionally `helyx-vector`.
- Copying it directly would also copy HELYX-specific operational assumptions
  into the Refine-Forge release surface.

`immortal-memory` is closer to Refine-Forge because it inherits edition `2021`
and license `MIT` from the NANTAR workspace. It is still not a drop-in
dependency because it depends on NANTAR workspace crates such as
`immortal-types`, `immortal-algebra`, `immortal-crypto`, `immortal-time`, and
optionally `immortal-vector`.

`consciousness-memory` inherits edition `2021` and license `MIT` from the
Consciousness-rs workspace. It is still not a drop-in dependency because it
depends on workspace crates such as `consciousness-algebra`, and its semantics
are tied to CONCIENCIA runtime concepts such as weighted token memory,
snapshot/core-memory persistence, faculties, evaluation receipts, and
claim-level gates.

## Enterprise Boundary

Central memory may provide:

- operator preferences,
- prior evidence indexes,
- source citations,
- role playbooks,
- task handoff context,
- non-authoritative retrieval for prompts and reports.

Central memory may not:

- set or upgrade `trust_level`,
- mark `human-reviewed`,
- remove blockers or warnings,
- mutate `runtime.policy_decisions`,
- replace CLI evidence receipts,
- convert external source material into claims without claim YAML and
  refinement evidence.

## Compatibility Contract

Any Refine-Forge memory backend should implement this logical record shape:

```json
{
  "schema_version": "refineforge-memory-v1",
  "id": "string",
  "agent": "lean|devops|train|kernel|run_all",
  "target": "helyx|cogn8ty|consciousness-rs|refine-forge|other",
  "kind": "preference|citation|evidence_index|handoff|claim_note|blocker",
  "content": "string",
  "source_path": "string|null",
  "source_sha256": "string|null",
  "created_at": "rfc3339",
  "trust_effect": "none"
}
```

`trust_effect` is intentionally fixed to `none`. Trust remains derived from
agent reports, claim files, refinement docs, release artifacts, and human
review evidence.

Refine-Forge now ships a backend-neutral JSONL implementation of this contract:

```bash
refine memory add --agent train --target helyx --kind citation \
  --content "Use llms-from-scratch-rs as fixture context only." \
  --source-path docs/agents/knowledge-source-audit.md

refine memory list --agent train --target helyx
refine memory import memory.jsonl
refine memory export memory-export.jsonl
```

The default store is `.refineforge/memory/records.jsonl` under `--root`.
Records are de-duplicated by deterministic `rfmem:<sha256>` ids. Source files
are hashed with SHA-256 when provided. The schema is
`schemas/memory-record.schema.json`.

## Recommended Path

1. Keep the open-source Refine-Forge default backend-neutral through
   `refine memory` JSONL import, list, and export.
2. Add optional internal bridge features that path-depend on
   `C:\HELYX\crates\helyx-memory` only in private HELYX deployments.
   `D:\AI-PROJECTS-GALO\PROJECTS\NANTAR INMORTAL RUST\crates\immortal-memory`
   is the better public-port seed if license ownership is confirmed.
   `D:\AI-PROJECTS-GALO\PROJECTS\Consciousness-rs-parity\crates\memory`
   can be bridged as a CONCIENCIA advisory/evidence-index source.
3. If a public vendored memory crate is required, port it deliberately:
   replace HELYX/NANTAR/Consciousness-only dependencies, resolve license
   ownership, preserve the source crate's safety posture, and run its bloom,
   graph, compression, store, provenance, and doctrine/evidence tests in
   Refine-Forge CI.

## Agent Use

- Lean agent: retrieve proof playbooks, citation notes, and prior model-only
  boundaries. Never promote a memory note into implementation refinement.
- DevOps agent: retrieve release audit history and operator policy. Never use
  memory as signed provenance.
- Training agent: retrieve dataset lineage, run notes, and benchmark context.
  Never use memory as model-quality evidence.
- Kernel agent: retrieve bitexact fixture notes and hardware run history.
  Never use memory as CUDA-correctness evidence.

## Consciousness-rs Advisory Use

`Consciousness-rs-parity` should be imported as a source of advisory records,
not as a trust backend:

- `target = "consciousness-rs"` for memory records derived from its docs,
  claims, bundles, crates, or evaluation output.
- `kind = "citation"` for source notes from `docs/evidence-standard.md`,
  `docs/claims-register.toml`, `CLAIMS.md`, or `README.md`.
- `kind = "evidence_index"` for references to `docs/refineforge/claims`,
  `docs/refineforge/lean`, and `docs/refineforge/bundles`.
- `kind = "handoff"` for operator notes connecting CONCIENCIA receipts to
  HELYX/COGN8TY agent work.

Those records can help the Lean agent audit external claim shape, the DevOps
agent track cross-project evidence standards, the Training agent harvest
evaluation-harness context, and the Kernel agent identify provenance or graph
fixtures. They do not prove Refine-Forge claims and do not upgrade agent trust.

This keeps Refine-Forge compatible with HELYX, COGN8TY, and Consciousness-rs
while preserving the central rule: memory can guide the agents, but evidence
classifies the agents.
