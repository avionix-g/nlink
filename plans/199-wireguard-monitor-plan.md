---
to: nlink maintainers
from: 0.19 second consolidation-pass — Connection<Wireguard> event subscription
subject: `Connection<Wireguard>::subscribe` + `WireguardEvent` + `into_events_with_resync` — GENL-family twin of Plans 185 + 191
status: queued for 0.19 — medium (closes WireGuard event-subscription gap)
target version: 0.19.0
parent: composes on Plan 196 (declarative WG) + Plan 195 (stream combinators)
source: Plan 191 §8b — was previously deferred to "0.20 separate plan"
created: 2026-05-30
---

# Plan 199 — `Connection<Wireguard>` event subscription

## 1. Why this plan exists

Plan 185 (0.18) shipped nftables watcher. Plan 191 (0.19)
adds the RTNETLINK twin. The WireGuard GENL family wasn't
in either's scope; Plan 191 §8 explicitly deferred it.

Under the 0.19 "everything in 0.19" directive (2026-05-30),
this plan pulls WireGuard event subscription into the cycle.
It's the third member of the watcher trinity:
- **nftables** (Plan 185) — ruleset mutations
- **RTNETLINK** (Plan 191) — link/addr/route/neigh/tc
- **WireGuard** (this plan) — peer mutations, handshake events

WireGuard's multicast surface is narrower than nftables or
RTNETLINK: the kernel emits an event when the peer set
changes (add/remove) and when allowed_ips change. Per-peer
handshake events ARE emitted; useful for monitoring connectivity.

## 2. The change

### 2.1 `WireguardGroup` enum

```rust
// crates/nlink/src/netlink/genl/wireguard/events.rs (new)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireguardGroup {
    /// Peer add/remove + allowed_ips changes.
    PeerChanges,
    /// Per-peer handshake events (received/initiated).
    Handshake,
    /// Convenience: subscribe to all.
    All,
}
```

Resolved via the shared GENL family multicast resolution
infra (Plan 156 / 99).

### 2.2 `WireguardEvent` enum

```rust
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum WireguardEvent {
    /// A peer was added to the device.
    PeerAdded { ifname: String, peer: WgPeer },
    /// A peer was removed.
    PeerRemoved { ifname: String, public_key: PublicKey },
    /// A peer's allowed_ips changed.
    PeerAllowedIpsChanged { ifname: String, public_key: PublicKey, new_allowed_ips: Vec<IpNet> },
    /// A handshake was received from a peer.
    HandshakeReceived { ifname: String, public_key: PublicKey, at: SystemTime },
    /// A handshake was initiated to a peer.
    HandshakeInitiated { ifname: String, public_key: PublicKey, at: SystemTime },
}
```

### 2.3 `impl EventSource for Wireguard`

Mirrors Plans 185 + 191. Parse-events fn dispatches on the
multicast nlmsg type + the carried command attribute.

### 2.4 `Connection<Wireguard>::subscribe` + `subscribe_all`

```rust
impl Connection<Wireguard> {
    pub fn subscribe(&mut self, groups: &[WireguardGroup]) -> Result<()>;
    pub fn subscribe_all(&mut self) -> Result<()>;
}
```

### 2.5 `wireguard_snapshot` + `into_events_with_resync`

Same shape as Plans 185 + 191. Snapshot walks
`get_device` for every WG interface (enumerate via
`Connection<Route>::get_links` filtered for kind="wg", or
via the WG family's own `GET_DEVICE` dump if it supports the
DUMP flag — verify in implementation).

```rust
pub async fn wireguard_snapshot(conn: &Connection<Wireguard>)
    -> Result<Vec<WireguardEvent>>
{
    let mut out = Vec::new();
    // For each WG interface, dump device + peers, emit as
    // PeerAdded events (representing "this peer exists now").
    ...
    Ok(out)
}

impl Connection<Wireguard> {
    pub fn into_events_with_resync(
        mut self,
        factory: ConnectionFactory<Wireguard>,
    ) -> Result<OwnedResyncStream> { ... }

    pub fn subscribe_all_with_resync(
        &mut self,
        factory: ConnectionFactory<Wireguard>,
    ) -> Result<BorrowedResyncStream<'_>> { ... }
}
```

## 3. Implementation phases

| Phase | Files | LOC |
|---|---|---|
| 1 — `WireguardGroup` + kernel-id mapping | new `genl/wireguard/events.rs` | ~60 |
| 2 — `WireguardEvent` enum (5 variants) | `events.rs` | ~60 |
| 3 — `parse_wireguard_event` dispatcher | `events.rs` | ~80 |
| 4 — `impl EventSource for Wireguard` | `stream.rs` | ~30 |
| 5 — `Connection<Wireguard>::subscribe` + `subscribe_all` | `genl/wireguard/connection.rs` | ~40 |
| 6 — `wireguard_snapshot` helper | new `genl/wireguard/resync.rs` | ~60 |
| 7 — `into_events_with_resync` + borrowed sibling | `genl/wireguard/resync.rs` | ~50 |
| 8 — Re-exports in `lib.rs` | `lib.rs` | ~5 |
| 9 — Recipe + example | new files | ~200 |
| 10 — Tests (see §4) | various | ~300 |
| **Total** | | **~885 LOC** |

## 4. Tests

### 4.1 Unit — `WireguardGroup::to_kernel_group`

```rust
#[test]
fn wireguard_group_resolution_is_dynamic() {
    // The WG GENL family looks up its mcast group id at
    // runtime (per-family registration). Test the resolution
    // path produces a non-zero group id when the kernel
    // supports WG (skip if it doesn't).
}
```

### 4.2 Unit — `parse_wireguard_event`

```rust
#[test]
fn parse_peer_added_dispatches_correctly() {
    let payload = build_peer_added_payload("wg0", test_pubkey(1));
    let evt = parse_wireguard_event(payload).unwrap();
    match evt {
        WireguardEvent::PeerAdded { ifname, peer } => {
            assert_eq!(ifname, "wg0");
            assert_eq!(peer.public_key, test_pubkey(1));
        }
        _ => panic!("expected PeerAdded"),
    }
}

#[test]
fn parse_handshake_event_extracts_timestamp() {
    let now = SystemTime::now();
    let payload = build_handshake_payload(now);
    match parse_wireguard_event(&payload).unwrap() {
        WireguardEvent::HandshakeReceived { at, .. } => {
            assert!(at.duration_since(now).unwrap_or_default() < Duration::from_secs(1));
        }
        _ => panic!(),
    }
}

#[test]
fn parse_unrecognized_event_returns_none() { ... }
```

### 4.3 Integration — root-gated + module-gated

```rust
#[tokio::test(flavor = "multi_thread")]
async fn subscribe_sees_peer_add_from_other_conn() -> Result<()> {
    require_root!();
    nlink::require_modules!("wireguard");

    let ns = TestNamespace::new("wg-events")?;

    // Create wg interface first.
    let route = namespace::connection_for::<Route>(ns.name())?;
    NetworkConfig::new().link(|b| b.wireguard().name("wg0"))
        .apply(&route).await?;

    let mut event_conn = namespace::connection_for_async::<Wireguard>(ns.name()).await?;
    event_conn.subscribe(&[WireguardGroup::PeerChanges])?;
    let mut events = event_conn.events();

    // From another connection, add a peer.
    let writer = namespace::connection_for_async::<Wireguard>(ns.name()).await?;
    let cfg = WireguardConfig::new("wg0")
        .peer(test_pubkey(1), |p| p.endpoint("10.0.0.1:51820".parse().unwrap()));
    cfg.apply(&writer).await?;

    // Drain the event stream.
    let evt = tokio::time::timeout(
        Duration::from_secs(3),
        events.next(),
    ).await
        .map_err(|_| nlink::Error::Timeout)?
        .expect("stream must yield")?;
    match evt {
        WireguardEvent::PeerAdded { peer, .. } => {
            assert_eq!(peer.public_key, test_pubkey(1));
        }
        other => panic!("expected PeerAdded; got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn wireguard_snapshot_walks_all_wg_interfaces() -> Result<()> {
    require_root!();
    nlink::require_modules!("wireguard");

    // Create wg0 with one peer. Create wg1 with two peers.
    // Snapshot must enumerate both.
    let snapshot = wireguard_snapshot(&conn).await?;
    let peer_count = snapshot.iter().filter(|e| matches!(e, WireguardEvent::PeerAdded { .. })).count();
    assert_eq!(peer_count, 3);
    Ok(())
}

#[tokio::test]
async fn into_events_with_resync_recovers_from_enobufs() -> Result<()> {
    // Mirror Plans 185 + 191's ENOBUFS recovery test, scoped
    // to WG events. Uses the same SO_RCVBUFFORCE technique +
    // flood-from-second-conn pattern.
    ...
}
```

## 5. Acceptance criteria

- [ ] `WireguardGroup` enum + dynamic kernel-id resolution.
- [ ] `WireguardEvent` with 5 variants.
- [ ] `impl EventSource for Wireguard`.
- [ ] `subscribe` + `subscribe_all`.
- [ ] `wireguard_snapshot` + `into_events_with_resync` +
      borrowed sibling.
- [ ] Re-exports at the crate root.
- [ ] 3+ unit tests + 3+ root-gated integration tests
      including ENOBUFS recovery.
- [ ] Recipe + example.
- [ ] CHANGELOG entry.

## 6. Effort estimate

| Phase | Effort |
|---|---|
| Code (~885 LOC) | ~4 h |
| Unit tests | ~1.5 h |
| Integration tests (kernel + ENOBUFS) | ~2.5 h |
| Recipe + example | ~1.5 h |
| CHANGELOG + migration guide | ~30 min |
| **Total** | **~10 h** |

## 7. Risks

- **WG kernel mcast group may not emit handshake events on
  older kernels** — the handshake notification mechanism
  has shifted over WG versions. Verify against the kernel
  version table; gate `WireguardGroup::Handshake` if
  needed.
- **ENOBUFS test repeatability** — same caveat as Plans 185
  + 191. The fix to `is_no_buffer_space()` (Plan 187 + 0.18
  Plan 185 fix) handles both `Kernel` and `Io` shapes; this
  plan inherits the fix automatically.
- **Snapshot walk performance**: on a node with many WG
  interfaces (rare but possible), the snapshot iterates
  each device + its peers. ~O(N×M) but bounded — flag if
  any consumer reports slowness.

## 8. Out-of-scope follow-ups

_None — this plan completes the WireGuard watcher coverage._

## 9. Cross-cutting artifacts

| Artifact | Action | Notes |
|---|---|---|
| `CHANGELOG.md` `## [Unreleased]` | **add** `### Added` entry for `Connection<Wireguard>` event subscription | Third member of the watcher trinity (nftables Plan 185, RTNETLINK Plan 191, WireGuard this plan). |
| `docs/migration_guide/0.18.0-to-0.19.0.md` | **append** `### Plan 199` section | Net-new feature. |
| `docs/recipes/wireguard-monitor.md` (**new**) | **create** ~140 lines | Walks ENOBUFS-resilient peer + handshake monitoring across a multi-WG-interface host. |
| `docs/recipes/README.md` | **add row** for `wireguard-monitor.md` | One line. |
| `crates/nlink/examples/wireguard/monitor.rs` (**new**) | **create** ~80-line demo | Subscribe + drain + handle ENOBUFS via `into_events_with_resync`. Register in `Cargo.toml`. |
| `README.md` `## High-Level APIs` | **add** "WireGuard Event Subscription" sub-section | Parallel to the nftables + RTNETLINK sub-sections. |
| `README.md` `## Library Modules` table | **update** the `nlink::netlink::genl::wireguard` row mention with event subscription | One line. |
| `CLAUDE.md` | **append** a note in the existing watcher / EventSource section: WireGuard is now in the EventSource impl set alongside Nftables, Route, Conntrack, etc. | Closes the watcher trinity. |

End of plan.
