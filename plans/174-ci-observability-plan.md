---
to: nlink maintainers
from: 0.16 cut post-mortem (2026-05-25)
subject: CI observability — `tracing-subscriber` init, modprobe completeness, ignored-tests visibility
status: proposed for 0.17 — three related improvements bundled into one plan
target version: 0.17.0
parent: 177-0.17-master-plan.md
source: Plan 167 §B+§C debug loop pain — would have been 1 CI iteration, not 3, with logs
created: 2026-05-25
---

# Plan 174 — CI observability

## 1. Why this plan exists

The Plan 167 cut activation took three push-watch-fix iterations
to localize the `send_batch` hang. The lib has `#[tracing::instrument]`
on every Connection method (per CLAUDE.md "Observability" section)
— but the integration test harness doesn't initialize a
`tracing-subscriber`, so all that instrumentation produced
exactly zero output in CI. We diagnosed the hang by writing
diagnostic *tests*, not by reading logs.

Three related gaps surfaced:

1. **No `tracing-subscriber` init** in `tests/integration.rs`.
2. **`nf_flow_table` not in the modprobe list** in
   `.github/workflows/integration-tests.yml`. The test passes
   because the kernel auto-loads on first netlink call, but
   the explicit list documents intent.
3. **13 pre-existing `#[ignore]`'d tests** in the suite (pre-
   Plan 166) with no visibility into what they are or why
   they're ignored.

## 2. The three fixes

### 2.1 Initialize `tracing-subscriber` in the integration harness

Add `tracing-subscriber` as a `[dev-dependencies]` entry, then
init once in `tests/integration.rs`:

```rust
// crates/nlink/tests/integration.rs

use std::sync::Once;

static SUBSCRIBER_INIT: Once = Once::new();

fn init_tracing() {
    SUBSCRIBER_INIT.call_once(|| {
        // EnvFilter reads RUST_LOG. Default "info" so the
        // suite is quiet unless explicitly debugged.
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info".into());
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_test_writer()  // routes to stdout captured by libtest
            .try_init();
    });
}
```

Update `common/mod.rs` `TestNamespace::new` (or `require_root!`)
to call `init_tracing()` so every test path triggers it.

Update `.github/workflows/integration-tests.yml`:

```yaml
- name: Run integration tests as root
  env:
    CARGO_TERM_COLOR: always
    RUST_LOG: "nlink=debug,nlink::netlink::nftables=trace"
  run: |
    cargo test -p nlink --features lab --test integration -- \
        --test-threads=1 --nocapture
```

**Effect**: when an integration test hangs or fails, CI logs
show the lib's debug trace including which method was in
flight at the hang point. Plan 167 step B+C iteration becomes
"read the log, fix, push" — 1 CI iteration instead of 3.

### 2.2 Add `nf_flow_table` to the CI modprobe list

`.github/workflows/integration-tests.yml` modprobes ~50
modules. `nf_flow_table` isn't there:

```yaml
# nftables
nf_tables nf_flow_table     # ← add nf_flow_table
nf_conntrack nf_conntrack_netlink
```

**Effect**: explicit dependency documentation. The flowtable
tests currently pass because the kernel auto-loads on first
`add_flowtable`, but a future env with `modprobe`-only loading
disabled (e.g., locked-down container) would silently fail.

### 2.3 Ignored-tests visibility

The suite ships 20 `#[ignore]`'d tests:
- 7 from Plan 166 `nftables_reconcile::*` (Plan 170 follow-up)
- 13 pre-existing (unknown reasons)

Add a `tests/integration/IGNORED.md` cataloging each ignored
test with:
- Test name
- Why ignored
- Tracking plan or issue
- How to run manually

Plus a CI workflow step that prints the count + flags any
ignored test that doesn't have a matching catalog entry:

```yaml
- name: Audit ignored tests
  run: |
    cargo test -p nlink --features lab --test integration -- \
        --list 2>/dev/null | grep ': test$' | wc -l
    # Cross-reference against tests/integration/IGNORED.md
    ./scripts/audit-ignored-tests.sh  # NEW
```

This shapes the "ignore" decision as a managed inventory rather
than a backdoor for hidden gaps.

## 3. Implementation phases

### Phase 1 — tracing-subscriber init

- `crates/nlink/Cargo.toml`: add
  `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`
  to `[dev-dependencies]`.
- `crates/nlink/tests/integration.rs`: add `init_tracing()`.
- `crates/nlink/tests/common/mod.rs`: call it in
  `TestNamespace::new`.
- `.github/workflows/integration-tests.yml`: add `RUST_LOG`
  env to the test step.

### Phase 2 — modprobe completeness

- One-line edit to `.github/workflows/integration-tests.yml`.

### Phase 3 — ignored-tests inventory

- Write `crates/nlink/tests/integration/IGNORED.md`.
- Write `scripts/audit-ignored-tests.sh` (mirrors the
  example-registration audit shape).
- Wire into `.github/workflows/rust.yml` as a new audit job.

## 4. Acceptance criteria

- [ ] `tracing-subscriber` initialized in integration tests;
      `RUST_LOG` env honored.
- [ ] CI step has `RUST_LOG=nlink=debug` so the next hang has
      logs.
- [ ] `nf_flow_table` in the modprobe list.
- [ ] `tests/integration/IGNORED.md` catalogues all 20
      ignored tests with reasons + tracking plans.
- [ ] `scripts/audit-ignored-tests.sh` exists + wired into CI;
      fails on any test ignored without a catalog entry.
- [ ] CHANGELOG entry under `### Changed`.

## 5. Effort estimate

| Phase | Effort |
|---|---|
| 1 tracing-subscriber | ~45 min |
| 2 modprobe + workflow edit | ~10 min |
| 3 IGNORED.md + audit script + CI wiring | ~45 min |
| **Total** | **~1.5 h** |

## 6. Risks

- **`tracing-subscriber` test-writer compatibility**: when
  running with `--test-threads=1 --nocapture`, the output goes
  to stdout. The `with_test_writer()` ensures libtest captures
  it consistently. Mitigation: spot-check locally first.
- **Pre-existing 13 ignored tests' rationale is lost**: some
  may have been added years ago with sparse comments. The
  IGNORED.md catalog has to either document why they're
  ignored OR remove them. Audit step in Plan 174.

## 7. Out-of-scope follow-ups

- **Kernel-version matrix CI**: would have caught Plan 170
  earlier (the GHA bookworm container behaves differently from
  the maintainer's Fedora). Cost: significant runner time per
  push. Plan 178 territory if 0.17 surfaces another
  kernel-specific issue.
- **Local sudo-test runbook**: a short doc on how to actually
  run the suite as root on a development machine. Low priority
  given the maintainer's "regular user" constraint.

End of plan.
