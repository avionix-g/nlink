---
to: nlink maintainers
from: nlink-lab upstream-asks report (2026-05-27) §Wishlist 2
subject: `Connection<Nftables>::subscribe_all_with_resync()` — bundles the nftables snapshot fn with the existing `events_with_resync` wrapper
status: queued for 0.18 — medium-size ergonomic helper
target version: 0.18.0
parent: (none — single-deliverable plan)
source: nlink-lab maintainer report `nlink-upstream-asks.md` §Wishlist 2 (nlink-lab Plan 158d motivates it)
created: 2026-05-27
---

# Plan 185 — `subscribe_all_with_resync` for nftables

## 1. Why this plan exists

`events_with_resync(stream, snapshot_fn)` already exists at
`nlink::events_with_resync` (Plan 151 closeout, shipped in
0.16). It takes a `Stream<Item = Result<T>>` and a snapshot
closure, transparently handles `ENOBUFS` recovery, and
returns a `Stream<Item = Result<ResyncedEvent<T>>>`.

The snapshot closure is the boilerplate. For nftables, every
caller writes substantially the same code:

```rust
async fn snapshot_dump(conn: &Connection<Nftables>) -> Result<Vec<NftablesEvent>> {
    let tables = conn.list_tables().await?;
    let mut events = Vec::new();
    for t in &tables {
        events.push(NftablesEvent::NewTable(t.clone().into()));
        for chain in conn.list_chains_in(&t.name, t.family).await? {
            events.push(NftablesEvent::NewChain(chain));
        }
        for rule in conn.list_rules(&t.name, t.family).await? {
            events.push(NftablesEvent::NewRule(rule));
        }
        for ft in conn.list_flowtables_in(&t.name, t.family).await? {
            events.push(NftablesEvent::NewFlowtable(ft));
        }
        for set in conn.list_sets_in(&t.name, t.family).await? {
            events.push(NftablesEvent::NewSet(set));
        }
    }
    Ok(events)
}
```

Every downstream consumer that wants ENOBUFS-resilient
nftables watching will write the same code. nlink-lab's
Plan 158d (per-namespace nftables watch) is the immediate
consumer; any controller doing "watch all nftables state
across many namespaces" wants the same.

## 2. The change

A new method on `Connection<Nftables>` that bundles the
subscription + the canonical snapshot fn:

```rust
// crates/nlink/src/netlink/nftables/connection.rs

impl Connection<Nftables> {
    /// Subscribe to the nftables multicast group with
    /// automatic `ENOBUFS` recovery. On overflow, dumps
    /// current state via
    /// `list_tables` / `list_chains_in` / `list_rules` /
    /// `list_flowtables_in` / `list_sets_in`, emits
    /// `Marker(ResyncStart)` + `Resynced(NewTable/Chain/Rule/...)`
    /// per item + `Marker(ResyncEnd)`, and resumes live events.
    ///
    /// Equivalent to manually wiring `events_with_resync` over
    /// `self.events()` with a snapshot closure that walks every
    /// table → chain/rule/flowtable/set. Bundled for the common
    /// case; for finer-grained snapshot scopes, wire
    /// `events_with_resync` directly.
    pub fn subscribe_all_with_resync(
        &mut self,
    ) -> Result<impl Stream<Item = Result<ResyncedEvent<NftablesEvent>>> + Send + 'static> {
        self.subscribe_all()?;
        let events = self.events();
        // Note: the closure needs a Connection — not `&self` —
        // because the snapshot is async + happens during
        // ENOBUFS recovery (potentially many seconds after
        // subscribe). Reopen a per-snapshot Connection from
        // the same socket parameters (pid? netns?). See §4
        // implementation notes for the wrinkle.
        let conn_factory = self.snapshot_factory()?;
        Ok(events_with_resync(events, move || {
            let conn = conn_factory.clone();
            async move { snapshot_nftables(&conn).await }
        }))
    }
}

async fn snapshot_nftables(
    conn: &Connection<Nftables>,
) -> Result<Vec<NftablesEvent>> {
    let mut events = Vec::new();
    let tables = conn.list_tables().await?;
    for t in &tables {
        events.push(NftablesEvent::NewTable(t.clone().into()));
        let chains = conn.list_chains_in(&t.name, t.family).await?;
        for c in chains { events.push(NftablesEvent::NewChain(c)); }
        let rules = conn.list_rules(&t.name, t.family).await?;
        for r in rules { events.push(NftablesEvent::NewRule(r)); }
        let fts = conn.list_flowtables_in(&t.name, t.family).await?;
        for f in fts { events.push(NftablesEvent::NewFlowtable(f)); }
        let sets = conn.list_sets_in(&t.name, t.family).await?;
        for s in sets { events.push(NftablesEvent::NewSet(s)); }
    }
    Ok(events)
}
```

### 2.1 The snapshot-Connection wrinkle

The snapshot closure runs during ENOBUFS recovery, on a
separate Connection from the one carrying the event stream.
Two reasons:

1. The event-stream Connection is busy receiving multicast
   frames; a synchronous dump on the same socket would
   interleave with multicast deliveries, surfacing the same
   `subscribe + unicast on same Connection` race that
   Plan 178's send_request/send_ack loop fix addressed.
2. The snapshot Connection needs to be in the same netns as
   the event-stream Connection. For ns-bound connections
   (`namespace::connection_for_async`), the closure has to
   know which netns.

Solution: a `SnapshotFactory` (or just a `String` netns
name + bound family/pid replay) that the method captures
and the closure clones each time it fires. The factory
opens a fresh Connection-per-snapshot via the same path
the original was opened.

This is the trickiest part of the plan — gets a careful
design pass before coding. The right API may be:

```rust
pub fn subscribe_all_with_resync_in(
    namespace: &str,  // explicit so factory knows
    snapshot_factory: impl Fn() -> Result<Connection<Nftables>> + Send + Sync + Clone + 'static,
) -> Result<impl Stream<...>>;
```

…leaving the convenience overload (default factory) for the
"current netns" case. Decide during implementation.

**Hard prerequisite**: Plan 181 (`list_*_in` family) must
land first — without `list_chains_in` / `list_flowtables_in`
/ `list_sets_in`, the snapshot fn has to either
- accept O(tables×entities) client-side filtering on a
  whole-namespace dump (slow on busy hosts), or
- ship simultaneously with Plan 181.

Bundle them as Plan 181 + Plan 185 in that order; 181 is a
hard prerequisite.

## 3. Tests

### 3.1 Unit — snapshot fn shape

```rust
#[test]
fn snapshot_nftables_yields_canonical_order() {
    // Build a known config via mocked Connection (or hit a
    // namespace if test isn't unit-pure).
    // Assert events come back in Table → Chain → Rule →
    // Flowtable → Set order per table.
}
```

### 3.2 Integration (root-gated)

```rust
#[tokio::test(flavor = "multi_thread")]
async fn subscribe_all_with_resync_recovers_from_enobufs() -> nlink::Result<()> {
    nlink::require_root!();
    nlink::require_modules!("nf_tables");

    // 1. Set up namespace with a known nft config (table + chain
    //    + rules + flowtable + set).
    // 2. Subscribe via subscribe_all_with_resync.
    // 3. Force ENOBUFS — flood the kernel with rules
    //    in a tight loop from another Connection while the
    //    consumer is slow.
    // 4. Assert the stream sees:
    //    a. Marker(ResyncStart)
    //    b. Resynced(NewTable/NewChain/NewRule/...) for each
    //       pre-existing item.
    //    c. Marker(ResyncEnd)
    //    d. Live events for the post-ENOBUFS state.
    // 5. Assert idempotence: no duplicate events for state
    //    that was both Resynced and re-observed via live.
}
```

This test pattern mirrors the existing
`events::test_*_with_resync` shape (Plan 151 closeout).

## 4. Acceptance criteria

- [ ] **Plan 181 is a hard prerequisite** — must land first.
- [ ] `Connection<Nftables>::subscribe_all_with_resync` exists.
- [ ] Snapshot fn covers all 5 entity kinds (tables, chains,
      rules, flowtables, sets) in canonical create-order.
- [ ] ENOBUFS recovery works end-to-end in the root-gated
      integration test.
- [ ] Snapshot-Connection factory design documented inline +
      in the rustdoc (the "why a separate Connection" rationale
      surfaces the race that motivated Plan 178).
- [ ] Recipe at `docs/recipes/nftables-watch-with-resync.md`
      walks a downstream consumer through the per-namespace
      pattern.
- [ ] CHANGELOG `### Added` entry.
- [ ] Migration-guide note.

## 5. Effort estimate

| Phase | Effort |
|---|---|
| API design (snapshot-factory shape) | ~45 min |
| Implementation | ~1 h |
| Unit test (snapshot order) | ~30 min |
| Integration test (ENOBUFS recovery) | ~1.5 h |
| Recipe + CHANGELOG + migration guide | ~30 min |
| **Total** | **~4 h** |

The integration test is the heavy item — forcing ENOBUFS
reliably across kernel versions is the same flakiness shape
Plan 151's existing tests already handle, so cross-reference
those for the technique.

## 6. Risks

- **Snapshot-Connection factory complexity**: closure captures
  a clonable factory; cloning a `Connection<Nftables>` isn't
  the right shape (sockets aren't `Clone`). The factory has
  to be `Arc<dyn Fn() -> Result<Connection<...>>>` or
  similar. Mock this out in the unit test before integration.
- **Multi-namespace consumers** (nlink-lab's actual use case):
  one subscription per namespace, fanned out. The plan only
  handles single-namespace; multi-ns fan-out is caller
  territory. Document explicitly.
- **`NftablesEvent::NewSet`** may not exist as a variant
  today — check `nftables/events.rs`. If it doesn't, either
  add it (small fix) or leave sets out of the snapshot for
  v1. Worth a 5-minute audit before scoping the plan as
  committed-shape.

## 7. Out-of-scope follow-ups

- **`subscribe_*_with_resync` for `Connection<Netfilter>`
  (conntrack)** — same pattern; conntrack's snapshot fn
  iterates `dump_conntrack`. Add when a consumer asks.
- **`subscribe_*_with_resync` for `Connection<Route>`** —
  rtnetlink link/addr/route/qdisc events. More complex
  snapshot (many entity kinds, ordering matters). Defer
  to a follow-up plan if downstream asks.
- **Per-table subscribe + snapshot** — finer scope. Defer.

## 8. Related future direction

If we end up adding `subscribe_*_with_resync` for 3+
protocols, factor into a `SnapshotResync` trait on
`Connection<P>` for any P with a known snapshot fn. Not
worth doing speculatively — the trait shape only becomes
clear once two concrete implementations exist.

End of plan.
