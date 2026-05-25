---
to: nlink maintainers
from: 0.16 cycle (Plans 153.1, 153.2, 153.3 + Plan 166)
subject: Hardware-only test coverage — strategy doc + proposed mitigation paths
status: proposed — documentation + strategy plan, no implementation work scheduled
target version: 0.17.0 (the strategy doc) — implementation paths are conditional on funding/hardware access
parent: 177-0.17-master-plan.md
source: Plan 166 §4 + Plan 167 §3.2 — XFRM offload / devlink rate / net_shaper caps tests have no CI path
created: 2026-05-25
---

# Plan 176 — hardware-only test coverage strategy

## 1. The gap

Three 0.16 features can't be validated under any current CI
environment because they require real hardware:

| Feature | What's needed | Test that's missing |
|---|---|---|
| **XFRM IPsec offload** (Plan 153.1) | NIC with crypto offload (mlx5, cxgb4, hns3, etc.) | `XfrmSaBuilder::offload()` round-trip |
| **Devlink rate limits** (Plan 153.2) | Devlink-capable NIC (most ConnectX cards) | `add_rate / set_rate / del_rate` round-trip |
| **Devlink port function state** (Plan 153.2) | SR-IOV-capable NIC with port-function support | `set_port_function_state()` round-trip — this is the one with the wire-format bug the 0.16 audit caught |
| **net_shaper caps** (Plan 153.3) | NIC with `net_shaper` driver support (kernel 6.13+, sparse driver list) | `get_caps()` round-trip |

Plan 166 §4 explicitly listed these as "out of scope for the
test backfill — they need real NICs no CI has."

## 2. Why this matters

The 0.16 audit caught the `set_port_function_state` wire-format
bug (attribute ID 174 vs. the correct 2) by code inspection,
not by test. If the lib ships a regression in any of these
paths, downstream users hit it before we do.

The lib code is correct *as written* for each — verified by
hand-trace against kernel UAPI. But "correct as written" is a
weaker guarantee than "passes integration test in CI."

## 3. Three mitigation paths

### 3.1 Self-hosted hardware runner

GitHub Actions supports self-hosted runners. Add one machine
with each of:

- **One Mellanox ConnectX-5+** (covers XFRM offload + devlink
  rate + port function — most coverage per box).
- **One i40e-based card** (Intel; covers a different driver
  path; useful for hash-function diversity).
- **One NIC with `net_shaper`** driver — mlx5 again, on a
  kernel ≥ 6.13.

Cost: one server (~$1000 used) + colo bandwidth + maintenance
time. Runs on every PR to master.

**Verdict**: probably overkill for a hobby crate; right call
once nlink has paying-customer downstreams or critical-
infrastructure adopters.

### 3.2 Vendor-provided cloud lab access

AWS / GCP / Azure offer SR-IOV-capable instances. A periodic
(weekly?) workflow spins up such an instance, runs the
hardware-gated suite, tears down. Workflow runs once per
release-cut, not per PR.

Cost: ~$50/cycle (instance hours during the test run). Manageable.

**Verdict**: feasible. Could be a Plan 178 if 0.17 surfaces a
hardware-touching regression.

### 3.3 Documented manual validation

The current pattern: maintainer runs the `--apply` examples
against their own dev hardware before each cut. Document the
exact validation steps in `docs/release-validation-manual.md`,
so it's a checklist, not tribal knowledge.

Cost: zero infra; maintainer time per cut.

**Verdict**: the realistic 0.17-timeframe answer. Pair with
explicit "no CI coverage" notes in the CHANGELOG for each
hardware-only feature.

## 4. Proposed Plan 176 deliverable for 0.17

Ship §3.3 only:

- `docs/release-validation-manual.md` — the maintainer's pre-
  cut hardware checklist:
  - Run `examples/xfrm/ipsec_*.rs --apply` if available.
  - Run any devlink rate example against a `--reset`-ed
    Mellanox dev machine.
  - Manually invoke `Connection::<NetShaper>::get_caps()` on a
    capable host; assert the response shape matches the lib's
    parser.
- CHANGELOG entries for any unverified-by-CI feature get an
  explicit "manually validated 2026-XX-XX against $HARDWARE"
  footnote so users can see the coverage shape.
- Plans 178+ enumerate §3.1/§3.2 paths in case future demand
  warrants them.

## 5. Acceptance criteria

- [ ] `docs/release-validation-manual.md` exists and walks
      every hardware-only feature shipped to date (Plans 153.1,
      153.2, 153.3).
- [ ] CHANGELOG conventions amended: hardware-only features
      get a `> ⚠ No CI coverage — manually validated against …`
      annotation under their entry.
- [ ] Plan 176 itself stays open until §3.1 or §3.2 has a
      concrete sponsor + scope.

## 6. Effort estimate

| Phase | Effort |
|---|---|
| 1 `release-validation-manual.md` first pass | ~45 min |
| 2 CHANGELOG annotation convention | ~15 min |
| 3 future-paths sketch (§3.1 + §3.2) | already in this plan |
| **Total** | **~1 h** |

## 7. Risks

- **`release-validation-manual.md` becomes stale**: every new
  hardware feature has to add an entry. Mitigation: PR
  template (or CONTRIBUTING.md) says "if your feature is
  hardware-only, update `docs/release-validation-manual.md`."
- **The manual validation is skipped**: cuts happen at speed,
  the checklist gets glanced at. Mitigation: `scripts/cut-
  release.sh` (Plan 175) can `cat docs/release-validation-
  manual.md` at the cut start as a forcing function.

## 8. Out-of-scope follow-ups

- **Plan 178** — self-hosted hardware runner (§3.1).
- **Plan 179** — vendor cloud lab cycle (§3.2).
- **NIC emulation** (e.g., QEMU virtio-net with `flow_offload`
  patch) — research path, not a near-term plan.

End of plan.
