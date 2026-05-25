---
to: nlink maintainers
from: Plan 174 §7 follow-up + Plan 178 closeout
subject: migrate 12 `#[ignore]`'d tests in `crates/nlink/tests/integration/diagnostics.rs` to `nlink::require_root!()`
status: queued for 0.18 — small mechanical bulk pass
target version: 0.18.0 (or sooner — no API impact)
parent: (none — single-deliverable plan)
source: Plan 174 §7 ("12 diagnostics.rs migration candidates"); IGNORED.md catalog
created: 2026-05-25
---

# Plan 179 — diagnostics.rs `#[ignore]` → `require_root!()` migration

## 1. The gap

Twelve tests in `crates/nlink/tests/integration/diagnostics.rs`
carry `#[ignore] // Requires root privileges for network
namespaces`. This is the pre-Plan-140 pattern: ignore the test
universally so non-root developer runs skip it, on the assumption
that the maintainer (a non-root user) won't run integration
tests in CI either.

The rest of the suite has long since moved to
`nlink::require_root!()` — a macro that early-returns `Ok(())`
when `euid != 0` so the test skips on non-root developer
machines AND runs for real in the privileged-CI job
(`.github/workflows/integration-tests.yml`, in tree since 0.15.0
via Plan 140).

The 12 `#[ignore]` tests miss the privileged-CI coverage they
should have. They're catalogued as "migration candidates" in
[`crates/nlink/tests/integration/IGNORED.md`](../crates/nlink/tests/integration/IGNORED.md)
under the diagnostics.rs section.

## 2. The migration

For each test (12 of them; full list in IGNORED.md), the change
is mechanical:

```rust
// Before
#[tokio::test]
#[ignore] // Requires root privileges for network namespaces
async fn test_diagnostics_scan() {
    let ns = TestNamespace::new("diag_scan").unwrap();
    // ... rest of test body ...
}

// After
#[tokio::test]
async fn test_diagnostics_scan() -> nlink::Result<()> {
    nlink::require_root!();
    let ns = TestNamespace::new("diag_scan")?;
    // ... rest of test body, with `?` instead of `.unwrap()` where useful ...
    Ok(())
}
```

Two adjacent cleanups while we're at each test:
1. Return type goes from `()` to `nlink::Result<()>` (matches
   the rest of the suite + lets `?` propagate setup errors).
2. `.unwrap()` calls swap to `?` where ergonomic. Don't chase
   every `.unwrap()` — only the ones that block readability.

The 12 test names:
- `test_diagnostics_scan`
- `test_diagnostics_scan_interface`
- `test_diagnostics_scan_interface_not_found`
- `test_diagnostics_check_connectivity_no_route`
- `test_diagnostics_check_connectivity_with_route`
- `test_diagnostics_find_bottleneck`
- `test_diagnostics_with_tc`
- `test_diagnostics_link_down_detection`
- `test_diagnostics_no_address_detection`
- `test_diagnostics_route_summary`
- `test_diagnostics_custom_config`
- `test_diagnostics_skip_loopback`

## 3. Risk: the tests may have latent bugs

These tests have never run anywhere (`#[ignore]` since they
were written, no CI before 0.15.0). Migrating them turns CI
green-or-red on their actual behavior. Some may fail — kernel
or `ip` command behavior may have drifted since the test was
written.

If a test fails post-migration:
1. Investigate the failure — is it a real bug in the
   `Diagnostics` API the test was supposed to catch, or a
   stale expectation in the test itself?
2. Fix the more-correct side (lib bug → fix lib; stale
   expectation → update test).
3. If a test is too brittle to fix in the migration pass,
   re-`#[ignore]` it with a Plan-179-followup reason in
   IGNORED.md and a `TODO` comment in the test body. **Don't**
   leave a failing CI just to ship the bulk migration.

Bound the risk: do the migration on a feature branch, push,
let CI run, address each failure on its own commit. If half
the tests fail, the plan splits into "land the easy 8" + "Plan
180 to investigate the 4 hard ones."

## 4. Acceptance criteria

- [ ] All 12 tests pass under `cargo test -p nlink --features
      lab --test integration` in privileged CI (or re-ignored
      with a documented reason if genuinely too brittle).
- [ ] `crates/nlink/tests/integration/IGNORED.md` updated:
      the diagnostics.rs section either removed entirely
      (success) or pared down to the survivors (partial).
- [ ] `scripts/audit-ignored-tests.sh` stays green.
- [ ] No CHANGELOG entry needed — this is test-only and
      surfaces no user-visible behavior change.

## 5. Effort estimate

| Phase | Effort |
|---|---|
| 1 mechanical migration (12 tests) | ~30 min |
| 2 first CI run + per-test failure triage | ~30 min – 2 h |
| 3 IGNORED.md cleanup | ~10 min |
| **Total** | **~1 – 3 h** depending on hidden-failure surface |

## 6. Why this is a separate plan rather than rolled into 174

Plan 174's scope was "CI observability" — the
`tracing-subscriber` init + `nf_flow_table` modprobe + IGNORED.md
catalog. Migrating the diagnostics.rs tests is a behavior
change (they go from never-running to running), which could
surface latent bugs and turn CI red. Bundling that into 174
would have either (a) blocked 174 on triaging unrelated
failures, or (b) hidden the migration risk by shipping a "the
tests stay ignored" interim.

Splitting keeps 174 a green-CI infra change and lets 179
take the migration risk on its own merits.

## 7. Out-of-scope follow-ups

- **Audit the rest of the suite for the same anti-pattern**:
  `grep -rn '#\[ignore\]\s*//' crates/nlink/tests/integration/`
  to find tests still using the comment-only-ignore shape.
  conntrack.rs's one ignored test uses `#[ignore = "..."]` with
  a real kernel-build-dependent reason — that one stays.

End of plan.
