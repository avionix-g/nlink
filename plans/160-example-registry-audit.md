---
to: nlink maintainers
from: nlink maintainers (audit triggered during the 0.16 cycle by a code review pass)
subject: Stale-example audit — 9 orphan example files + CI gap
status: investigation complete; per-file fixes deferred for maintainer judgement
target version: 0.16.0 (audit) / 0.17.0 (fixes if not handled at 0.16 cut)
parent: 146-0.16-master-plan.md
created: 2026-05-24
---

# Plan 160 — example-registration audit + CI gap remediation

## Why this exists

A code-review pass during the 0.16 cycle (after Plan 156 landed)
discovered **9 example files under `crates/nlink/examples/` that
are not registered as `[[example]]` entries** in
`crates/nlink/Cargo.toml`. Since Cargo only auto-discovers examples
at the top level of `examples/` (one level deep), every file in a
subdirectory must be declared explicitly. Without that declaration:

- `cargo build --workspace --all-targets` doesn't compile them.
- `cargo run --example <name>` can't invoke them.
- API drift accumulates silently — there's no compile signal.

All 9 files fail to compile against the current API. **None of the
failures are 0.16 regressions** — verified per-symbol via
`git log -S '<symbol>' -- crates/nlink/src/` that the referenced
APIs either (a) were renamed long before 0.16 or (b) never existed
in tree at all. The examples were written speculatively against
design notes that the implementation never matched.

The 0.16 cycle does **not** ship a fix for the examples themselves
— each one needs per-file judgement that's better made when
someone has time to read the file and decide "update vs delete".
What ships in 0.16 is the **safety net** that prevents this from
recurring:

- `scripts/audit-example-registration.sh` (in tree, dormant —
  see "Wiring" below).
- This plan as the catalog + recommendation.
- A `CLAUDE.md` convention note that future examples MUST be
  registered.

## Per-file catalog

Categories (per the 0.16 audit):

- **R = Rename-only** — symbol just needs a one-token edit
  (e.g., `link_kind()` → `kind()`); the example was right once,
  drifted on a rename. Possibly fixable in <10 minutes.
- **F = Format-string bug** — example uses
  `println!(r#"...{}..."#)` where the raw-string body contains
  `{}` placeholders the format-string parser tries to consume.
  Mechanical fix (`println!("{}", r#"..."#)` or escape braces).
- **P = Phantom API** — references symbols / fields / methods
  that never existed in tree. The example was written from a
  design doc the implementation diverged from. Needs either a
  full rewrite or deletion.
- **O = Obsolete shape** — references an API shape that was
  replaced (e.g., the `LinkConfig` struct-based API replaced by
  the closure-based `NetworkConfig::link(name, |b| ...)`
  builder). Rewrite required.

| File | Category | Notes |
|---|---|---|
| `bridge/fdb.rs` | P | `LinkMessage::link_kind()` never existed (use `.kind()`). `FdbEntry::is_local()` never existed (type carries `is_self` / `is_master` / `is_extern_learn` / `is_permanent`). Speculative write that didn't match what shipped. |
| `bridge/vlan.rs` | R + F | `.link_kind()` → `.kind()` — single occurrence. Plus 7 raw-string printlns whose bodies contain `{}` placeholders. |
| `config/declarative.rs` | O | Whole example is written against a struct-based API (`LinkConfig`, `AddressConfig`, `RouteConfig`, `QdiscConfig`) that the `nlink::netlink::config` module never exposed. The actual API is closure-based: `NetworkConfig::new().link(name, |b: LinkBuilder| …)`. Full rewrite required. |
| `diagnostics/bottleneck.rs` | P + F | Reads `bottleneck.score` — `Bottleneck` (diagnostics.rs:344) has `location` / `bottleneck_type` / `current_rate` / `drop_rate` / `total_drops` / `recommendation`; no `score`. |
| `diagnostics/connectivity.rs` | P | Reads `route.dev` and `route.src` — `RouteInfo` (diagnostics.rs:329) has `destination` / `prefix_len` / `gateway` / `oif` / `metric`. Also treats `gateway_reachable` as `Option<bool>` but it's a plain `bool`. |
| `diagnostics/scan.rs` | P + F | Reads `iface.up` / `iface.carrier` as bools — actual field is `state: OperState`. Reads `RouteDiag::has_default_v4` — actual field is `has_default_ipv4` (always was). |
| `route/mpls.rs` | R + F | `route.gateway` → `route.via`. Plus 9 raw-string printlns with format placeholders. |
| `route/nexthop.rs` | R + F | `nh.is_blackhole()` → `nh.blackhole` (field, not method). Plus 8 raw-string printlns with format placeholders. |
| `route/srv6.rs` | P + F | Reads `route.table` — `Srv6LocalRoute` (srv6.rs:363) has `sid` / `prefix_len` / `action` / `oif` / `iif` / `protocol`; no `table` ever existed. |

**Verdict per file:**

- **Trivially fixable (R + F)** — `bridge/vlan.rs`, `route/mpls.rs`,
  `route/nexthop.rs`. Maintainer should pick a sitting and
  knock these out together; each is ~20 minutes of mechanical
  edits (rename + raw-string `println!("{}", r#"..."#)` pass +
  register in Cargo.toml).
- **Rewrite required (P / O)** — the other 6. Recommendation:
  decide which still demonstrate something useful and rewrite
  those; delete the rest. `config/declarative.rs` is the most
  valuable to rewrite (declarative-config is a real headline
  API) and the most expensive (full file rewrite). The
  diagnostics trio are mostly `println!`-of-doc-strings —
  arguably better replaced by the `--apply` runner pattern
  documented in `CLAUDE.md` (Active work).

## CI-gap analysis

The CI workflow at `.github/workflows/rust.yml` runs:

```yaml
build-and-test-default-features:
  - run: cargo build --workspace --all-targets
  - run: cargo test --workspace
build-and-test-all-features:
  - run: cargo build --workspace --all-targets --all-features
  - run: cargo test --workspace --all-features
```

`--all-targets` includes examples — but **only the examples
Cargo knows about**, i.e., the ones declared in `Cargo.toml` plus
top-level `examples/*.rs`. Files in subdirectories without an
explicit `[[example]]` entry are silently invisible to cargo. So
"CI is green" + "the 9 orphans don't compile" coexist consistently.

The existing `audit-examples` job (`rust.yml:136`) runs
`scripts/audit-example-features.sh`, which iterates over the
`[[example]]` blocks in `Cargo.toml` to check feature-gating. It
also inherits the registration blind spot — only sees registered
examples.

## Wiring (the safety net)

`scripts/audit-example-registration.sh` is in tree. It walks
`crates/nlink/examples/` recursively, finds every `*.rs`, and
verifies each is referenced by a `path = "..."` line in
`crates/nlink/Cargo.toml`. Exits 0 if every file is registered;
exits 1 with a per-file list otherwise.

The script is **NOT yet wired into the CI workflow** (no job
stanza in `.github/workflows/rust.yml`) because the 9 known
orphans would immediately break CI on the 0.16 branch. The
proposed two-step rollout:

1. **Catalog phase (this commit, in 0.16)** — script in tree;
   not in CI. Maintainer can run `bash
   scripts/audit-example-registration.sh` locally to verify the
   catalog matches reality.

2. **Enforcement phase (post-catalog-resolution)** — after the
   9 orphans are resolved (each either fixed-and-registered or
   deleted), add the workflow stanza below to `rust.yml` and
   the script becomes a mandatory CI gate:

   ```yaml
   audit-example-registration:
     name: audit example registration (every .rs in examples/ is in Cargo.toml)
     runs-on: ubuntu-latest
     steps:
       - uses: actions/checkout@v4
       - name: Run scripts/audit-example-registration.sh
         run: bash scripts/audit-example-registration.sh
   ```

   This new job runs in ~3s (pure bash, no toolchain), so it's
   essentially free.

## Acceptance criteria

For this plan to close:

- [x] `scripts/audit-example-registration.sh` ships in tree.
- [x] This plan documents the 9 orphans + per-file category.
- [x] CLAUDE.md project conventions note registers
      "every example .rs must have an `[[example]]` entry".
- [ ] Maintainer triages the 9 orphans (one or more follow-up
      commits, one per file or grouped).
- [ ] Post-triage: workflow stanza added to `rust.yml`; the audit
      becomes a mandatory CI gate.

## Cross-references

- `scripts/audit-example-registration.sh` — the safety-net script.
- `scripts/audit-example-features.sh` — sibling script (different
  concern: required-features gating for the examples that ARE
  registered).
- `.github/workflows/rust.yml` — where the workflow stanza lands
  in the enforcement phase.
- `CLAUDE.md` — convention note added in the same commit as this
  plan.
