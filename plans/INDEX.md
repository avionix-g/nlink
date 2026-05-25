---
subject: nlink plan index + progress tracker (0.17 cycle)
status: live (update as PRs land)
target version: 0.17.0
maintainer: p13marc
created: 2026-05-25 (rewritten from the 0.16 cycle tracker after the 0.16 cut)
---

# Plan index — 0.17 cycle

Day-to-day tracker for nlink's 0.17 cycle. The 0.16 cycle's
per-plan scaffolding (Plans 146 – 168) was deleted post-cut
per project convention (the CHANGELOG `## [0.16.0]` section
and `docs/migration_guide/0.15.1-to-0.16.0.md` carry the
durable narrative). Plan 169's Phases 1+2 shipped in 0.16;
its Phase 3 (`Bottleneck::score`) was rewritten as a slim
design plan and lives at
[`169-bottleneck-score-design.md`](169-bottleneck-score-design.md).

## Quick status

- **Cycle**: 0.17.0 — branched from master (= 0.16.0 head)
  2026-05-25.
- **Branch**: all 0.17 work pushes to the `0.17` branch.
  Cycle cut → master merge happens at release time. **Do not
  push to master.**
- **Workspace version**: still `0.16.0`. Bump to `0.17.0`
  after the first 0.17-breaking change lands (Plan 171 is the
  likely trigger).
- **CI**: a new draft PR `0.17 → master` will drive CI on
  every push (same workflow as the 0.16 cycle's PR #3 —
  GitHub Actions only triggers on master push/PR).

## Status legend

| Symbol | Meaning |
|---|---|
| ⚪ | Planned — not started |
| 🟡 | In progress — PR open or work underway |
| 🟢 | Merged to master |
| 🔵 | Cut & published |

## Sub-plan table

Master plan: [177](177-0.17-master-plan.md) — cycle theme,
sequencing rationale, scope boundaries.

| Plan | Title | Effort | Order | Status | PR(s) | Notes |
|------|-------|--------|-------|--------|-------|-------|
| [170](170-nft-send-batch-hang-investigation-plan.md) | `Connection::<Nftables>::send_batch` seq filter + end-seq termination — fixes the 0.16 cut's CI hang; un-ignores 7 `nftables_reconcile::*` tests | ~2.5 h | 1 | ⚪ | – | Full forensic written during the 0.16 cut. CI evidence definitive (run `26405827382`). Lib path correct on local Fedora kernel; hang specific to GHA `rust:bookworm` container. |
| [171](171-default-connection-timeout-plan.md) | Default 30s operation timeout on `Connection<P>` — opt-out via `.with_no_timeout()`; closes the "hidden hang" class that masked Plan 170 | ~3 h | 2 | ⚪ | – | Semver: behavior-changing for pathological callers. Documented in migration guide. Number aligns with the integration suite's existing 30s explicit cap. |
| [172](172-recv-loop-audit-plan.md) | Audit + harden every recv-loop in the lib for the Plan 170 hang pattern — 9 loops total, 8 already structurally defensive (just need Plan 171's timeout wrap); 1 (`send_batch`) is the Plan 170 fix | ~2 h | 3 | ⚪ | – | Depends on Plan 170 (canonical fix shape) + Plan 171 (timeout helper). |
| [173](173-parse-error-from-impls-plan.md) | `From<AddressParseError>` + `From<RouteParseError>` for `nlink::Error` — removes the `.map_err()` ceremony in `NetworkConfig` caller chains | ~30 min | 4 | ⚪ | – | Surfaced by `examples/config/declarative.rs` rewrite. Pure additive; landed via thiserror `#[from]` on new enum variants. |
| [174](174-ci-observability-plan.md) | `tracing-subscriber` in integration test harness + `nf_flow_table` modprobe + ignored-tests catalog | ~1.5 h | 5 | ⚪ | – | Would have made Plan 170 a 1-CI-iteration debug, not 3. Logs the lib's existing `#[tracing::instrument]` output during CI runs. |
| [175](175-release-cut-tooling-plan.md) | `scripts/cut-release.sh` orchestrating the cut sequence + handling the `cargo publish --dry-run` inversion, CHANGELOG promotion, GitHub release length truncation | ~2 h | 6 | ⚪ | – | Three friction points hit during the 0.16 cut. Script handles each explicitly. |
| [176](176-hardware-test-coverage-plan.md) | Hardware-only test coverage strategy doc (XFRM offload / devlink rate / net_shaper caps) — §3.3 deliverable for 0.17; §3.1 (self-hosted) and §3.2 (cloud lab) are future-plan sketches | ~1 h doc | 7 | ⚪ | – | The 0.16 audit caught a devlink wire-format bug by code inspection only; this plan formalizes the "manual validation before cut" workflow + sets up future infra paths. |
| [169 Phase 3](169-bottleneck-score-design.md) | `Bottleneck::score: f64` normalized severity — combines drop_rate (weight 0.6) + backlog pressure (0.3) + error rate (0.1) | ~1 h | 8 | ⚪ | – | Design call carried forward from 0.16. Formula sketched in the plan. |

Total 0.17 focused-work estimate: **~13 hours** + CI cycle
time + migration-guide write-up for the 0.16.0 → 0.17.0
transition.

## Deprioritized (parked)

| Plan | Why parked |
|------|------------|
| [152](152-0.16-integration-showcases-plan.md) | `aya` co-demo + Prometheus exporter + OTel example. Carried forward from 0.16 without a real adopter signal. Revisit if a downstream asks for it. |

## How to update this file

When a plan moves status:

1. Edit the **Status** column emoji.
2. Edit the **PR(s)** column — comma-separated list of PR
   numbers, or "—" if not yet open.
3. When a plan ships, optionally add a one-line outcome note
   to **Notes** (e.g., "shipped commit abc1234").
4. When the cycle cuts, the relevant rows become 🔵 and
   per-plan files get deleted (per the post-cut convention
   established at 0.16; substance lives in CHANGELOG +
   migration guide).
