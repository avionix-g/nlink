---
subject: nlink plan index — 0.20 cycle seed
status: 0.19.0 shipped 2026-05-31 (crates.io + GitHub release); 0.20 cycle open, no plans written yet
last updated: 2026-05-31
---

# Plan index

Day-to-day plan tracker. Per `CLAUDE.md ## Publishing` /
`Plan-file cleanup`, plan files are working memory and get
deleted when a cycle cuts. The durable narrative lives in
`CHANGELOG.md` + `docs/migration_guide/`.

## 0.19.0 — shipped 2026-05-31

`nlink@0.19.0` + `nlink-macros@0.19.0` published to crates.io;
`v0.19.0` tag + GitHub release on master. Durable narrative:

- [`CHANGELOG.md ## [0.19.0]`](../CHANGELOG.md)
- [`docs/migration_guide/0.18.0-to-0.19.0.md`](../docs/migration_guide/0.18.0-to-0.19.0.md)

Headline: F1 (Connection serialization under `Arc`), four
CRITICAL wire-format defects (nft verdicts, XFRM SP, devlink
mcast), build-time sizeof CI gate, NetworkConfig correctness
pass, DPLL `phase_offset` widened to `i64`.

## Active plans (carrying past 0.19)

| Plan | Status | Notes |
|------|--------|-------|
| [197](197-declarative-ovpn-plan.md) | deferred to 0.20 | Kernel 6.16+ ovpn GENL UAPI; needs imperative `Connection<Ovpn>` family + scoped implementation effort. Link half shipped via Plan 190 §2.3b. |

## 0.20 cycle seed

Topics worth scoping when work picks up. None have plan files
yet — write them as the cycle takes shape.

| Topic | Source |
|-------|--------|
| Plan 197 — ovpn GENL family imperative + declarative | `plans/197-declarative-ovpn-plan.md` |
| Plan 205 follow-on — wire purge correctly with kernel-managed-resource exclusion list | `CHANGELOG.md ## [0.19.0]` Plan 205 deferral note |
| F1 follow-on — full NlRouter-style dispatcher task (Mutex serialization shipped in 0.19 Plan 194; dispatcher unlocks per-request pipelining + multicast-events vs request safety) | `docs/migration_guide/0.18.0-to-0.19.0.md` §"Plan 194" |
| Plan 208 Phase 3-4 — GENL command unification + family-resolution unification (15th recv-loop closeout: `wg_command` stale-frame race) | `CHANGELOG.md ## [0.19.0]` finding H9 |
| Plan 189 §8 expansions — `Deserialize` + `schemars` JSON Schema | `docs/migration_guide/0.18.0-to-0.19.0.md` §"Plan 189" |
| Plan 193 phase 2-3 — `cargo-fuzz` infrastructure + `proptest` integration | `docs/migration_guide/0.18.0-to-0.19.0.md` §"Plan 193" |
| Plan 195 — `StreamBackoff` + `Store<K>` reflector + `backon` | `docs/migration_guide/0.18.0-to-0.19.0.md` §"Plan 195" |
| Plan 196 follow-ups — `WireguardConfig::client()` shortcut + `from_wg_config()` INI parser | `docs/migration_guide/0.18.0-to-0.19.0.md` §"Plan 196" |
| Plan 198 — full declarative `DeclaredSet` + element diff | `docs/migration_guide/0.18.0-to-0.19.0.md` §"Plan 198" |
| Plan 201 — broader sweep (`From`/`Into` + `Display` + `#[inline]` on builders) | `docs/migration_guide/0.18.0-to-0.19.0.md` §"Plan 201" |
| Audit follow-ups — H7 (`ip vrf exec` real impl) + H8 (`ip xfrm` lib wire-up) | `CHANGELOG.md ## [0.19.0]` findings H7/H8 |

## How to update this file

1. When a cycle opens, add the new plan rows + a "Cycle X.Y"
   section at the top.
2. When a plan ships and the cycle cuts + publishes, delete
   the per-plan file in the cut commit. The CHANGELOG entry
   + migration-guide section carry the durable narrative.
3. Keep this file slim — it's a pointer, not an archive.
