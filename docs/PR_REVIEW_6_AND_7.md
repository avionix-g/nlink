# Pull Request Review — PR #6 & PR #7

**Reviewer:** post-0.19 audit pass
**Date:** 2026-05-31
**Branch under review:** `master` (both PRs target master)
**Local context:** the `0.19` branch is 39 commits ahead of master; this review treats both PRs as merge candidates against master, then flags 0.19-cycle interaction risk.

---

## Executive summary

| PR | Subject | Type | Recommendation | Urgency |
|---|---|---|---|---|
| [#7](https://github.com/p13marc/nlink/pull/7) | TC filter `tcm_info` packing — `priority<<16 \| htons(proto)` | **bug fix** (correctness) | **MERGE FIRST.** Live-kernel-confirmed `EINVAL` on every TC filter add with explicit protocol+priority. Two-line semantic fix on three pack sites + accessor symmetry restored + 4 new unit tests + 1 root-gated integration test. | 🔴 **critical** — affects every user calling `add_filter_*` with `protocol` ≠ 0. |
| [#6](https://github.com/p13marc/nlink/pull/6) | IPv6 SNAT/DNAT helpers (`snat_v6` / `dnat_v6`) | **feature** + breaking | **MERGE SECOND.** Adds missing v6 NAT surface + closes a silent-dropped-register bug (v6 NATs had no `Ipv4Addr` to carry → encoder skipped `NFTA_NAT_REG_ADDR_MIN`). All CI green. One breaking-change rename (`NatExpr.addr` field type). | 🟡 **important** — silently broken IPv6 NAT path is a real correctness gap, but it's behind an API consumers have to explicitly reach for. |

Both PRs ship with strong test coverage (unit + root-gated integration). Both can merge cleanly; ordering is recommended for semver and migration-guide hygiene, not because of mechanical conflict.

---

## PR #7 — `fix(tc): pack filter tcm_info as the kernel expects`

- **Author:** Denys Fedoryshchenko (Collabora) — first contribution
- **Branch:** `nuclearcat:fix/tc-filter-tcm-info-packing` → `master`
- **Files:** 5 (`+180/-39`)
- **Mergeable:** ✅ yes (no conflicts vs master)
- **CI:** ⏳ **not yet run** (`statusCheckRollup: []`). Recommend nudging — likely just a missed trigger.

### What's broken on master today

`crates/nlink/src/netlink/filter.rs:3536` (and 3 sibling sites for `replace` / `change` / `delete`) packs:

```rust
// tcm_info = (protocol << 16) | priority   ← WRONG
let info = ((protocol as u32) << 16) | (priority as u32);
```

The kernel encodes `tcm_info` as `TC_H_MAKE(prio << 16, htons(proto))` — see `include/uapi/linux/pkt_sched.h` and `net/sched/cls_api.c::tc_new_tfilter`:

- **Priority** in the **upper 16 bits** (the "major" half).
- **Ethernet protocol** in the **lower 16 bits**, in **network byte order** (`htons(ethertype)`).

The kernel then compares the protocol against `skb->protocol` (also network-byte-order) when walking per-protocol classifier dispatch. With the halves transposed *and* no `htons`, calling e.g. `add_filter_full(..., 0x0800, 200, ...)`:

1. Kernel reads protocol = `200` and priority = `0x0800`.
2. Protocol `200` is not a known ethertype → `EINVAL`.
3. Every TC filter add with an explicit protocol fails.

**Verified live** by the author on kernel 6.17 — pre-fix: errno 22; post-fix: `Ok` and `tc filter show` lists `protocol ip pref 200`.

### Compound problem: self-inconsistent accessors

On master, `TcMessage::protocol()` / `priority()` use the *buggy* unpack:

```rust
pub fn protocol(&self) -> u16 { (self.header.tcm_info >> 16) as u16 }      // returns priority bits
pub fn priority(&self) -> u16 { (self.header.tcm_info & 0xFFFF) as u16 }   // returns proto bits, no ntohs
```

…while the **unused** `filter_protocol()` / `filter_priority()` had the kernel-correct unpack. The unit test `test_filter_protocol_priority` baked in the buggy layout — so it passed but enforced the wrong contract.

### The fix

| Site | Before | After |
|---|---|---|
| `TcMsg::with_filter_info` (new) | — | `with_info(((priority as u32) << 16) \| (protocol.to_be() as u32))` |
| `filter.rs` add/replace/change/delete | `with_info(((protocol as u32) << 16) \| (priority as u32))` | `with_filter_info(protocol, priority)` |
| `ratelimit.rs` ingress filter | same buggy inline pack | `with_filter_info(0x0003, 1)` |
| `TcMessage::protocol()` | upper 16 (priority bits) | `u16::from_be((tcm_info & 0xFFFF) as u16)` |
| `TcMessage::priority()` | lower 16, no ntohs | `(tcm_info >> 16) as u16` |
| `filter_protocol()` / `filter_priority()` | duplicated unpack | thin `Option` wrappers over `protocol()` / `priority()` (one source of truth) |

The single `TcMsg::with_filter_info(protocol, priority)` packer is the chokepoint — every TC-filter call site (4 in `filter.rs` + 1 in `ratelimit.rs`) routes through it. This is the right shape.

### Test coverage

| File | Tests | Coverage |
|---|---|---|
| `types/tc.rs` (NEW `#[cfg(test)] mod tests`) | 3 tests | `with_filter_info_matches_iproute2_wire_format` pins the iproute2 byte layout (`info == 0x0064_0008` on LE for `proto=0x0800, prio=100`). `with_filter_info_round_trips_priority_and_protocol` iterates 4 (proto, prio) pairs. `pre_fix_layout_was_transposed` is an **explicit regression guard** documenting what the kernel parsed pre-fix. |
| `messages/tc.rs` | replaced existing test | Now packs via `with_filter_info` then asserts `protocol() / priority() / filter_protocol() / filter_priority() / info()` all decode to host-order values with kernel-correct bit positions. |
| `tests/integration/tc.rs` | 1 root-gated test | `test_filter_add_explicit_protocol_priority` — adds a real `cls_matchall` filter with `ETH_P_IP, prio 200`. Pre-fix: `EINVAL`; post-fix: accepted. Gated by `require_root!() + require_modules!("sch_htb", "cls_matchall")`. |

The regression-guard test (`pre_fix_layout_was_transposed`) is particularly well-shaped — it bakes in *what was wrong* as an asserted record, so anyone re-introducing the swap will hit a self-explaining failure.

### Semver implications

The accessor return values **change behavior** for messages with non-zero `tcm_info`:

- `TcMessage::protocol()` previously returned the priority value (interpreted as `u16`); now returns the actual ethernet protocol.
- `TcMessage::priority()` previously returned `htons(protocol)` (interpreted as `u16`); now returns the actual priority.

Function signatures are unchanged (both still `pub fn(&self) -> u16`), so `cargo semver-checks` won't flag this — but it's a **runtime semantic break**. Any downstream code that was reading the *buggy* values and relying on them would silently get different numbers post-fix.

Realistically, **no consumer can be relying on the buggy values for anything correct** (they were arbitrary garbage that didn't match what the kernel stored or what `tc(8)` prints). Document in the CHANGELOG under `### Fixed` with the explicit "if you were reading these accessors, they now return the values the kernel actually holds" note.

### Surface gaps in this PR

| Gap | Severity | Disposition |
|---|---|---|
| **No `CHANGELOG.md` entry**. | low | Add one on merge — typical maintainer-side polish. The PR body explains the fix well enough. |
| **No migration guide note**. | low | Only matters if a 0.19 migration guide is open; the 0.19 branch has one. Add a paragraph there if landing on `0.19`. |
| **`get_filters` dump limitation called out but not addressed**. | n/a | The PR explicitly says read-back is flaky because `RTM_GETTFILTER` dump with `tcm_ifindex = 0` returns nothing on modern kernels. This is a separate nlink issue, not blocking. Worth opening as a separate tracking issue. |
| **CI hasn't fired** (`statusCheckRollup: []`). | medium | Push an empty commit or close/reopen to nudge — needs green before merge. |

### Recommendation

**Merge after CI green** with a CHANGELOG `### Fixed` entry. This is a clean, contained, well-tested correctness fix. The `pre_fix_layout_was_transposed` regression test is exactly the kind of self-documenting guard that pays for itself the first time someone refactors the packer.

---

## PR #6 — `feat(nftables): IPv6 SNAT/DNAT rule helpers`

- **Author:** avionix-g — repeat contributor (PR #1 merged, PR #2 closed)
- **Branch:** `avionix-g:ipv6-rule-snat` → `master`
- **Files:** 4 (`+310/-34`)
- **Mergeable:** ✅ yes
- **CI:** ✅ **all 12 checks green** (integration, build+test default, build+test all-features, clippy, docs, semver-checks, public-api diff, example feature/registration audits, ignored-tests audit, machete, msrv).

### What's broken on master today

`Rule::dnat` / `Rule::snat` are IPv4-only. The `Expr::Nat` encoder at `nftables/expr.rs:163` emits `NFTA_NAT_REG_ADDR_MIN` based on `if nat.addr.is_some()` — but `nat.addr` is `Option<Ipv4Addr>`. A user wanting v6 NAT has two paths today, both broken:

1. **Construct `NatExpr` manually + push an `Immediate` R0 load**: must leave `addr = None` (there's no `Ipv4Addr` to put there). The encoder then skips `NFTA_NAT_REG_ADDR_MIN`. The kernel sees an R0 load with no NAT expr consuming it → the NAT happens against an unset register → wrong / dropped traffic.
2. **No public helper at all**: the `dnat` / `snat` builders only accept `Ipv4Addr`.

The bug is silent (no `EINVAL`) — the rule installs but doesn't NAT.

### The fix

Three coordinated changes:

**(1) Type-level: `NatExpr.addr: Option<Ipv4Addr>` → `NatAddr` enum**

```rust
pub enum NatAddr {
    None,             // port-only NAT or pre-loaded register
    V4(Ipv4Addr),     // v4 — record the value for future dump/decode
    Reg,              // R0 is loaded with a 16-byte address (or anything non-v4); no Ipv4Addr to record
}

impl NatAddr {
    pub fn reg_in_use(&self) -> bool { !matches!(self, NatAddr::None) }
}
```

Encoder changes to `if nat.addr.reg_in_use()`. This makes the illegal-but-previously-representable state (`addr: Some(...)` but somehow no register load) unrepresentable.

**(2) Refactor: `Rule::push_nat` helper consolidates the 3-step load pattern**

```rust
fn push_nat(&mut self, nat_type, family, addr_bytes, addr: NatAddr, port) {
    debug_assert!(addr.reg_in_use(), "push_nat always loads R0; ...");
    self.exprs.push(Expr::Immediate { dreg: R0, data: addr_bytes });
    if let Some(p) = port { self.exprs.push(Expr::Immediate { dreg: R1, data: p.to_be_bytes() }); }
    self.exprs.push(Expr::Nat(NatExpr { nat_type, family, addr, port }));
}
```

This is the right shape — the invariant *"a register-in-use `NatAddr` always has a real `R0` load preceding it"* lives in one place, debug-asserted.

**(3) New `Rule::{snat_v6, dnat_v6}` builders**

```rust
pub fn snat_v6(mut self, addr: Ipv6Addr, port: Option<u16>) -> Self {
    self.push_nat(NatType::Snat, Family::Ip6, addr.octets().to_vec(), NatAddr::Reg, port);
    self
}
// dnat_v6 identical with NatType::Dnat
```

Note the deliberate choice: emits `Family::Ip6` in the NAT expr (not the chain's `Family::Inet`). This is correct per the nftables UAPI — the NAT expr's family must match the address family.

### Test coverage

| File | Tests | Coverage |
|---|---|---|
| `nftables/types.rs` test module | 3 new tests | `dnat_v6_loads_address_and_marks_register` — pins the 16-byte R0 immediate, `nat.addr == NatAddr::Reg`, `nat.family == Family::Ip6`. `dnat_v6_with_port_loads_proto_register` — verifies the R1 port load. `snat_v6_loads_address_and_marks_register` — the SNAT counterpart. The existing `nat.addr == Some(...)` assertion is migrated to `NatAddr::V4(...)`. |
| `tests/integration/nftables_reconcile.rs` | 2 new root-gated tests | `dnat_v6_rule_round_trips` and `snat_v6_rule_round_trips`. **These use the diff-idempotency pattern** — `cfg.diff(&nft).await?.apply(&nft).await?` then re-diff and assert `again.is_empty()`. This is **far stronger** than "expression_bytes is non-empty": an empty re-diff means the kernel stored byte-for-byte what nlink emitted. SNAT and DNAT validated on separate hooks (postrouting/`SrcNat` vs prerouting/`DstNat`). |

The diff-idempotency assertion is the gold standard for nftables round-trip testing and matches exactly what Plan 157 §2.6's `apply_reconcile` machinery was designed to enable.

### Semver implications

**Breaking** (explicitly documented in the PR description + commit message):

- `NatExpr.addr` field changed from `Option<Ipv4Addr>` to `NatAddr`.
- Struct-literal construction breaks (`NatExpr { addr: None, ... }` → `NatExpr { addr: NatAddr::None, ... }`).
- Pattern matching on `addr` breaks (`Some(ip)` → `NatAddr::V4(ip)`, plus a new `Reg` arm to handle).
- Builder APIs unchanged: `NatExpr::snat(family) / dnat(family) / .addr(Ipv4Addr)` and `Rule::snat / dnat` all keep their signatures + behavior. v4 wire output is byte-identical.

`cargo-semver-checks` will flag the field-type change. CHANGELOG entry already says `### Changed` (breaking).

### Strengths

1. **The `NatAddr` enum is the right abstraction.** It models the actual kernel-side invariant ("an address register is in use, with or without a value to record") instead of overloading `Option<Ipv4Addr>` to mean two unrelated things.
2. **`debug_assert!` on the `push_nat` invariant** catches future contributors who add a NAT helper without loading R0 first.
3. **Integration tests use the round-trip diff pattern** instead of raw byte assertions. This catches *anything* the kernel parses differently from what nlink emitted — endianness, attribute ordering, missing attrs.
4. **The PR description is exemplary** — explains the bug clearly, explains why the breaking change is the right shape, offers to rework if the maintainer prefers.

### Possible concerns

| Concern | Severity | Disposition |
|---|---|---|
| `NatAddr::V4(...)` records an `Ipv4Addr` value that the **encoder never reads back** (the encoder only checks `reg_in_use()`). The comment says "Recorded for a future dump/decode path; no decoder currently reads it back." | low | Acceptable forward-compat. Worth opening a tracking issue for the eventual `RTM_GETRULE` decode path. |
| Why `Family::Ip6` and not the chain's `Family::Inet`? The PR explains this is per UAPI but it's worth a one-liner module doc somewhere prominent. | very low | Existing rustdoc on `snat_v6`/`dnat_v6` already calls this out. Could lift to a `NatExpr`-level note. |
| No `NatAddr::V6(Ipv6Addr)` variant — v6 ends up as `NatAddr::Reg` losing the address. | low | Symmetric to v4 would be nicer, but the encoder doesn't need it (16-byte address is already in the `Immediate` R0 load). Future dump-side decode will need to surface it somehow. |

None of these are blocking.

### Recommendation

**Merge.** Clean, well-tested, breaking-change is unavoidable + minimally invasive. The PR ships exactly the diff-idempotency integration coverage that the 0.19 audit identified as the gold standard.

---

## Interaction with the in-flight `0.19` branch

Both PRs target `master`. The `0.19` branch is 39 commits ahead; merging the PRs to master then rebasing/merging the 0.19 branch later is the natural order. Conflict landscape:

| File | PR | 0.19 touched? | Conflict? |
|---|---|---|---|
| `CHANGELOG.md` | PR #6 | yes (heavily — 0.19 `## [Unreleased]` block) | **trivial conflict** — both add to `## [Unreleased]`. Keep both blocks. |
| `crates/nlink/src/netlink/nftables/types.rs` | PR #6 | yes (Plan 189 serde derives + Plan 198 SetKeyType extensions + `prefix_to_mask`) | **non-overlapping line ranges** — PR #6 hits the `NatExpr` / `NatType` region (lines ~290-340 + tests at ~1485); 0.19 hits `Family/Hook/Priority` (~5-130) + `SetKeyType` (~1290-1370) + new helper (~1441). Should auto-merge clean. |
| `crates/nlink/src/netlink/nftables/expr.rs` | PR #6 | no (one line in 0.19 — Plan 189 serde derive only) | none |
| `crates/nlink/tests/integration/nftables_reconcile.rs` | PR #6 | no | append-only at the bottom — clean |
| `crates/nlink/src/netlink/filter.rs` | PR #7 | no | none |
| `crates/nlink/src/netlink/messages/tc.rs` | PR #7 | no | none |
| `crates/nlink/src/netlink/ratelimit.rs` | PR #7 | no | none |
| `crates/nlink/src/netlink/types/tc.rs` | PR #7 | no | none |
| `crates/nlink/tests/integration/tc.rs` | PR #7 | no | none |

**Cleanest sequencing:**

1. **Land PR #7 first** (master). Critical correctness fix; deserves to ship out of band of the 0.19 cycle.
2. **Land PR #6 second** (master). The breaking-change requires a CHANGELOG entry in `## [Unreleased]` which is exactly where the 0.19 entries will land too.
3. **Rebase / merge the 0.19 branch over master** after both land. The 0.19 backfill integration tests (`cycle_0_19_backfill.rs`) don't overlap with either PR's test files.
4. **Cut 0.19.0** with both fixes folded in. Both fixes ship in the `0.19.0` release notes as part of the cycle.

Alternative path if 0.19 cuts first: cherry-pick both PRs into a 0.19.1 patch release. Less clean — recommend (1)+(2)+(3)+(4).

### Recommended CHANGELOG entries

For PR #7 (after the 0.19 `### Fixed` block):

```markdown
- **TC filter `tcm_info` packing — kernel-EINVAL on every `add_filter*`
  with explicit protocol+priority** — `add_filter_by_index_full` (and
  the sibling `replace`/`change`/`delete` paths) packed `tcm_info` as
  `(protocol << 16) | priority` with no `htons`. The kernel uses
  `TC_H_MAKE(prio << 16, htons(proto))` — priority in the upper 16
  bits, ethernet protocol in the lower 16 bits in network byte order.
  Result: every TC filter add with an explicit ethernet protocol was
  rejected with EINVAL; ratelimit's ingress filter was silently
  installed under the wrong ethertype. Single chokepoint
  `TcMsg::with_filter_info(protocol, priority)` now owns the packing;
  `TcMessage::protocol()` / `priority()` accessor semantics now match
  the kernel (previously returned the transposed values). 4 new unit
  tests pin the iproute2 wire layout + add a regression guard; 1
  root-gated integration test asserts a real filter add accepts.
  Verified on kernel 6.17.
```

For PR #6 (in `### Changed` — breaking):

```markdown
- **IPv6 SNAT/DNAT (`Rule::snat_v6` / `Rule::dnat_v6`) + `NatExpr.addr`
  re-typed to `NatAddr` enum** — v6 NAT was silently broken: the
  encoder emitted `NFTA_NAT_REG_ADDR_MIN` only when `nat.addr.is_some()`,
  but v6 NATs have no `Ipv4Addr` to put there. New helpers load the
  16-byte address into `R0`, the optional port into `R1`, and emit
  `Family::Ip6` in the NAT expr (matching the address family, not the
  chain's `Family::Inet`). `NatExpr.addr: Option<Ipv4Addr>` becomes
  `NatAddr` enum (`None` / `V4(Ipv4Addr)` / `Reg`) — modeling
  "register in use" and "the IPv4 value to record" as one value
  makes the illegal state unrepresentable. **Breaking** for code
  constructing `NatExpr` as a struct literal or matching on `addr`;
  the `Rule::{snat,dnat,snat_v6,dnat_v6}` and
  `NatExpr::{snat,dnat,.addr()}` builders are unaffected. v4 wire
  output is unchanged.
```

---

## Suggested merge actions

1. **PR #7**: nudge CI (push empty commit or close/reopen), confirm green, request a CHANGELOG entry, merge.
2. **PR #6**: already green, request the maintainer-side polish (verify the existing CHANGELOG entry survives the 0.19 merge), merge.
3. **Open follow-up tracking issues**:
   - PR #7 `get_filters` `tcm_ifindex = 0` dump-returns-nothing limitation
   - PR #6 future `RTM_GETRULE` decode of NAT expr → expose `NatAddr` value (v4 + v6)

---

*Report ends. Both PRs are well-formed, well-tested, and ready for merge after the noted nits.*
