---
to: nlink maintainers
from: 0.16 post-cut audit (2026-05-25)
subject: Audit + harden every `recv_msg().await?` loop in the lib for the Plan 170 hang pattern
status: proposed for 0.17 (depends on Plan 170 + Plan 171 landing first; uses them as the canonical fix shape)
target version: 0.17.0
parent: 177-0.17-master-plan.md
source: Plan 170 forensic + grep across the lib (42 `recv_msg().await?` sites; 9 in recv-loops; 1 missing seq filter)
created: 2026-05-25
---

# Plan 172 — recv-loop hang-pattern audit

## 1. Why this plan exists

Plan 170 found that `Connection::<Nftables>::send_batch` has
three structural issues in its response-handling loop: no
nlmsg_seq filter, returns on the first ACK rather than the
batch-end's ACK specifically, and no operation timeout. That
combination caused the 22-minute CI hang that bracketed the
0.16 cut.

The natural follow-up question: **is `send_batch` the only
loop with these problems?** The whole lib follows the same
shape — `recv_msg().await?` in a `loop { ... }` driven by
the kernel's response protocol. Each loop has the same hang
potential if the design isn't defensive.

This plan does the audit and proposes a fix-per-loop.

## 2. The audit (preliminary findings)

Grep across `crates/nlink/src/netlink/`:

```
$ grep -rn "recv_msg().await" crates/nlink/src/netlink/ | wc -l
42
```

42 call sites total. Of these:

- **Single-call sites** (~25): `let data = recv_msg().await?;`
  outside a loop. These can't hang in the multi-message sense
  but can hang on the single call. Plan 171's default
  operation timeout covers them.
- **Recv-in-loop sites** (~9): the dangerous shape. Enumerated
  below.

### 2.1 Recv-in-loop site inventory

| File | Line | Loop driver | Seq filter? | End condition | Status |
|---|---|---|---|---|---|
| `connection.rs` | 410 (`send_dump_inner`) | rtnetlink dump | ✓ Yes (:429) | `is_done` | ✅ Defensive |
| `connection.rs` | 902 (`subscribe_route`) | multicast events | by group | per-msg dispatch | ✅ N/A — open stream |
| `connection.rs` | 2503 (anon) | similar to send_dump | ✓ Yes (:2509) | `is_done` | ✅ Defensive |
| `nftables/connection.rs` | 614 (`send_batch`) | batch commit | ✗ **No** | first ACK | ❌ **Plan 170 — the known bug** |
| `nftables/connection.rs` | 713 (`nft_dump`) | nft dump | ✓ Yes (:720) | `is_done` | ✅ Defensive (but no timeout — Plan 171) |
| `genl/wireguard/connection.rs` | 303 | wg dump | ✓ Yes (:310) | `is_done` | ✅ Defensive |
| `genl/macsec/connection.rs` | 495 | macsec dump | ✓ Yes (:502) | `is_done` | ✅ Defensive |
| `genl/mptcp/connection.rs` | 472 | mptcp dump | ✓ Yes (:479) | `is_done` | ✅ Defensive |
| `genl/ethtool/connection.rs` | 1180 | ethtool dump | ✓ Yes (:1187) | `is_done` | ✅ Defensive |
| `genl/devlink/connection.rs` | 535, 593, 622 | devlink dumps (3 sites) | ✓ Yes (:542/600/629) | `is_done` | ✅ Defensive |
| `genl/nl80211/connection.rs` | 594 | nl80211 dump | ✓ Yes | `is_done` | ✅ Defensive |

**Summary**: **`send_batch` is the only loop missing the seq
filter** (Plan 170). Every other recv-loop is structurally
defensive against the wrong-seq-flood failure mode. But none
of them have a timeout — so if the kernel never sends a DONE
marker (e.g., a kernel bug, or a containerized environment
that drops it), every loop hangs forever. **Plan 171** is the
mitigation.

### 2.2 What the audit does NOT catch

- **Multicast subscription paths** (`subscribe_route`,
  `subscribe_links`, etc.) — these are open-ended streams by
  design; no DONE marker expected. They consume one message at
  a time. Plan 171's per-call timeout still applies (each
  `recv_msg().await` can hang if the multicast source dies).
  This isn't a bug; it's the multicast contract.
- **Dump-stream paths** (Plan 149's `dump_stream`) — these are
  caller-driven `Stream<Item = T>` impls; the consumer drives
  the timeout via `tokio::time::timeout(stream.next())`. Lib-
  side default is the fallback.

## 3. Per-loop work

Given §2.1's finding that only `send_batch` is structurally
broken (and that's Plan 170), this plan's per-loop work
reduces to:

### 3.1 Apply Plan 171's `recv_with_timeout` helper

After Plan 171 introduces `Connection<P>::recv_with_timeout()`
(or equivalent wrap helper), every loop in §2.1 switches to
it. Mechanical edit, no behavior change for non-pathological
kernels.

### 3.2 Add a unit test per loop family that asserts timeout

Mock socket. Loop construction. Assert `Error::Timeout` after
the configured duration. Loop families:
- `send_dump_inner` (rtnetlink-style)
- `nft_dump`
- GENL-family dump (parameterized)

One test per family; covers the per-family loops by structure
since they're all macro-derived or trait-implemented after
Plan 156 / Plan 154.

### 3.3 Add CLAUDE.md note on the loop shape

Reinforce that the canonical recv-loop shape is:

```rust
let seq = self.socket().next_seq();
loop {
    let data = self.recv_with_timeout().await?;
    let mut done = false;
    for msg in MessageIter::new(&data) {
        let (header, payload) = msg?;
        if header.nlmsg_seq != seq {
            continue;          // ← seq filter is mandatory
        }
        if header.is_error() {
            // ... error handling ...
        }
        if header.is_done() {
            done = true;
            break;
        }
        // ... payload accumulation ...
    }
    if done { break; }
}
```

Future contributors adding new recv-loops have a clear template.

## 4. Acceptance criteria

- [ ] Audit table (§2.1) confirmed accurate and complete.
- [ ] Every loop in §2.1 routes through Plan 171's
      `recv_with_timeout` helper.
- [ ] 3 unit tests (one per family) asserting `Error::Timeout`
      after the configured duration when the mock socket never
      responds.
- [ ] CLAUDE.md gains the canonical loop-shape note (§3.3).
- [ ] CHANGELOG entry under `### Changed` referencing the
      audit closeout.

## 5. Effort estimate

| Phase | Effort |
|---|---|
| 1 mechanical edit (apply helper at 9 sites) | ~30 min |
| 2 unit tests (3 families) | ~45 min |
| 3 CLAUDE.md amendment | ~10 min |
| 4 CHANGELOG | ~10 min |
| **Total** | **~1.5 – 2 h** |

## 6. Dependencies

- **Plan 170** must land first — provides the canonical "fixed
  loop" shape that the audit applies elsewhere.
- **Plan 171** must land first — provides the
  `recv_with_timeout` helper this plan distributes.

## 7. Risks

- **The audit misses a recv site** because the grep `recv_msg().await`
  excludes alternate idioms (`self.fd.read(...)`, custom paths).
  Mitigation: grep for `tokio::io::AsyncReadExt::read` and
  `AsyncFd::readable` to catch any I/O-layer recv that isn't
  going through `recv_msg`.
- **The "this loop is structurally defensive" verdicts in §2.1
  are wrong for some loops** — e.g., a per-family loop that
  has the seq filter but does something unusual with the
  payload that creates a hang in a different way. Mitigation:
  each loop review reads ±20 lines of context before declaring
  it defensive. Spot-checked at audit-write time.

## 8. Out-of-scope follow-ups

- **Multicast subscription liveness probes**: `Connection<P>`
  could send a periodic `NLMSG_NOOP` (or no-op equivalent) on
  multicast sockets to detect a silently-dead peer. Plan 178
  territory if customers actually report this.
- **`recvmmsg` batching's timeout semantics**: `recv_batch` is
  the per-syscall recv used under `syscall_batch`. Same Plan
  171 timeout applies to each call, but the batch-aggregate
  behavior may need its own analysis. Spot-check during Plan
  172 execution.

End of plan.
