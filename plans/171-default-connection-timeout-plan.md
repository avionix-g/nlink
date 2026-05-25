---
to: nlink maintainers
from: 0.16 cycle CI evidence (2026-05-25)
subject: Default operation timeout on `Connection<P>` — close the "hidden hang" class
status: proposed for 0.17 (semver implication needs maintainer sign-off on default value)
target version: 0.17.0
parent: 177-0.17-master-plan.md
source: Plan 170 deep-debug — the underlying lib bug + the silent-hang failure mode that masked it
created: 2026-05-25
---

# Plan 171 — default operation timeout on `Connection<P>`

## 1. The problem

`Connection::<P>::timeout(Duration)` is opt-in. The default is
`None` → no timeout → every `recv_msg().await` can block
forever. CLAUDE.md documents this:

> Operation timeouts are opt-in via `Connection::timeout(Duration)`;
> default is none.

That documentation note is correct, but it doesn't surface the
*consequence*: any bug that causes the kernel to send fewer
datagrams than the lib expects (Plan 170 is the canonical
example, found by accident in CI) manifests as an indefinite
hang. Manual cancellation is the only escape. CI runners get
terminated by job-level wall-clock limits; humans wait, get
frustrated, kill the process.

This isn't theoretical — Plan 170 cost the 0.16 cycle three
push-watch-fix CI iterations to localize, including 22 minutes
of GHA runner time burned on a single hung test.

## 2. The proposal

Change the `Connection<P>` default from `None` to **30
seconds**. Existing `timeout(Duration)` and `with_no_timeout()`
(new) calls let callers override.

```rust
// Before
let conn = Connection::<Route>::new()?;  // no timeout

// After
let conn = Connection::<Route>::new()?;  // 30s default

// Existing override (unchanged behavior on its current call sites):
let conn = Connection::<Route>::new()?.timeout(Duration::from_secs(5));

// New escape hatch for the "I really do need to wait forever" case:
let conn = Connection::<Route>::new()?.with_no_timeout();
```

## 3. Why 30 seconds

The number must balance:

- **Long enough** that legitimate slow operations don't trip it.
  rtnetlink dumps on huge route tables can take 5-10s; nft
  transaction commits on thousands of rules can take 2-3s on
  busy hosts.
- **Short enough** to flag hangs while they're still useful
  debugging signals. 30s is a CI wall-clock that fails fast
  enough to keep `gh run watch` interactive; long enough that
  almost no real operation reaches it.

The Plan 166 integration tests already use 30s as their
explicit per-test cap — see `nftables_reconcile.rs` `with_timeout()`
helper. Aligning the lib default with that established budget
keeps the mental model consistent.

Reference points from other crates:
- `reqwest` default: 30s (HTTP client; analogous "one
  request-response" budget).
- `tokio::net::TcpStream`: no default (caller responsibility);
  but `tokio::time::timeout` is the canonical idiom.
- `neli`: no default (matches our current state; same hang
  potential).

## 4. Semver impact

Changing a default from "infinity" to "30s" is technically
behavior-changing. Pathological callers that:

- Call `conn.get_routes().await?` expecting it to block forever
  if the kernel never responds, AND
- Don't currently call `.timeout()` to set their own value, AND
- Are doing this intentionally rather than by accident…

…will see `Error::Timeout` after 30s instead of an indefinite
block. The behavior change is observable. Per Cargo's 0.x semver
rules, the next minor bump (0.16 → 0.17) is the right place.

**Documented as a breaking change** in
`docs/migration_guide/0.16.0-to-0.17.0.md`:

> `Connection<P>` now defaults to a 30s operation timeout.
> If your code relied on infinite waits (rare), call
> `.with_no_timeout()` to restore the previous behavior.

## 5. Implementation

### 5.1 Lib changes

```rust
// crates/nlink/src/netlink/connection.rs

const DEFAULT_OPERATION_TIMEOUT: Option<Duration> =
    Some(Duration::from_secs(30));

impl<P: ProtocolState> Connection<P> {
    pub fn new() -> Result<Self> /* ... existing body ... */
        .map(|conn| Connection {
            timeout: DEFAULT_OPERATION_TIMEOUT,
            ..conn
        })
    }

    /// Set a custom operation timeout. Replaces the default 30s.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Opt out of the default operation timeout. Use sparingly
    /// — without a timeout, any kernel response anomaly hangs
    /// the call indefinitely. See [Plan 171] for why this is
    /// not the default.
    pub fn with_no_timeout(mut self) -> Self {
        self.timeout = None;
        self
    }
}
```

### 5.2 Apply at every `recv_msg().await?` site

Wrap every recv site in `tokio::time::timeout(self.timeout, ...)`:

```rust
async fn recv_with_timeout(&self) -> Result<Vec<u8>> {
    match self.timeout {
        Some(duration) => tokio::time::timeout(
            duration,
            self.socket.recv_msg()
        ).await.map_err(|_| Error::Timeout)?,
        None => self.socket.recv_msg().await,
    }
}
```

There are 42 `recv_msg().await?` sites. Most route through
`send_dump_inner` / `send_request` helpers, so the helper-level
wrap covers them. Per-family GENL connections have their own
recv loops (wireguard / macsec / mptcp / ethtool / nl80211 /
devlink) — each needs the same wrap.

Plan 172's audit shares the same call-site list, so 171's
implementation and 172's audit can be one pass.

### 5.3 CLAUDE.md amendment

Replace:

> Operation timeouts are opt-in via `Connection::timeout(Duration)`;
> default is none.

with:

> `Connection<P>` defaults to a 30-second operation timeout. Use
> `.timeout(Duration)` to change it, or `.with_no_timeout()` to
> opt out. The default exists to prevent "hidden hang" failures
> (Plan 170) — any kernel response anomaly surfaces as
> `Error::Timeout` after 30s instead of blocking indefinitely.

## 6. Acceptance criteria

- [ ] `Connection<P>` defaults to 30s timeout.
- [ ] `.timeout(Duration)` overrides; `.with_no_timeout()` opts
      out.
- [ ] Every `recv_msg().await?` in the lib is wrapped in the
      configured timeout (via helper).
- [ ] Existing tests pass without modification (30s budget is
      generous enough).
- [ ] New unit test: construct a `Connection` against a mock
      socket that never responds; assert `Error::Timeout` after
      the configured duration.
- [ ] CHANGELOG entry under `### Changed` (breaking).
- [ ] Migration guide entry documents the override path.
- [ ] CLAUDE.md amendment lands.

## 7. Effort estimate

| Phase | Effort |
|---|---|
| 1 lib-side `recv_with_timeout` helper + wrap sites | ~1.5 h |
| 2 unit test (mocked socket) | ~30 min |
| 3 CHANGELOG + migration-guide entries | ~30 min |
| 4 CLAUDE.md amendment | ~10 min |
| 5 verify existing tests still green | ~30 min |
| **Total** | **~3 hours** |

## 8. Risks

- **30s too short for legitimate cases**: the slowest legitimate
  op observed in tests is ~5s. 30s is a 6× margin. Mitigation:
  ship; if a real workload trips it, the user's `.timeout(60.s)`
  override is the fix.
- **Unit-testable without a kernel?**: yes — wrap a mock socket
  that never sends. Don't need root.
- **Per-family connections (Wireguard, etc.)**: each has its
  own recv site. Audit per Plan 172 ensures every site is
  wrapped.
- **`AsyncFd` cancellation safety**: `tokio::time::timeout` is
  drop-cancel safe; if the timeout fires, the inner
  `recv_msg().await` future is dropped, which cancels the
  underlying I/O. Standard pattern.

## 9. Out-of-scope follow-ups

- **Per-method timeouts** (different defaults for dumps vs.
  mutations vs. multicast subscribe): nice-to-have but adds
  surface. Plan 178 territory if 30s-everywhere proves
  insufficient.
- **`AsyncFd` interrupt-on-signal**: separate concern; the
  default timeout doesn't address Ctrl-C cleanup.

End of plan.
