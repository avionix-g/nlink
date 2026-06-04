# 0.20.1 batch C — changelog handoff

These four plans (227, 228, 230, 231) ship the typed-API tightening
cluster for 0.20.1 in additive-only form. The corresponding deprecated
symbols are listed under a dedicated "Deprecated" subsection per plan
so the cut-time CHANGELOG editor can pivot them into a single block.

## Plan 227 — `AddressFamily` newtype + typed `Connection<Route>` siblings

### Added

- `nlink::AddressFamily` newtype (re-exported from
  `nlink::util::address_family::AddressFamily`). Construct via
  `AddressFamily::v4()` / `v6()` / `bridge()` / `mpls()` / `packet()` /
  `unspec()` (with `ipv4()` / `ipv6()` aliases) or `from_raw(u8)` for
  unmodelled bytes. Implements `From<AddressFamily> for u8`, `Display`,
  `is_known()` discriminator.
- `Connection<Route>::get_rules_for_family_typed(AddressFamily) ->
  Vec<RuleMessage>` — typed sibling. `AddressFamily::unspec()` returns
  the unfiltered dump (matches kernel `AF_UNSPEC` meaning).
- `Connection<Route>::del_rule_by_priority_typed(AddressFamily, u32)`.
- `Connection<Route>::flush_rules_typed(AddressFamily)`.

### Deprecated

- `Connection<Route>::get_rules_for_family(family: u8)` — use
  `get_rules_for_family_typed(AddressFamily)`.
- `Connection<Route>::del_rule_by_priority(family: u8, priority: u32)` —
  use `del_rule_by_priority_typed(AddressFamily, u32)`.
- `Connection<Route>::flush_rules(family: u8)` — use
  `flush_rules_typed(AddressFamily)`.

The raw-`u8` forms silently no-op for unmodelled family bytes (the
per-family filter yields zero matches); the typed siblings make the
input boundary type-checked.

## Plan 228 — typed `Percent` on declarative `QdiscBuilder.loss`

### Added

- `QdiscBuilder::loss_pct(crate::util::Percent) -> Self` — typed
  sibling for setting netem packet-loss percentage on the
  declarative builder. Internally delegates through
  `Percent::as_percent()` so the stored `Option<f64>` state matches
  the deprecated path for sane inputs; `Percent::new` clamping kills
  the units-confusion footgun at the typed boundary.

### Deprecated

- `QdiscBuilder::loss(percent: f64)` — use
  `loss_pct(Percent::new(x))`. The raw-`f64` form silently accepts
  out-of-range and NaN; the typed sibling clamps to `[0, 100]` and
  rejects non-finite inputs at construction.

### Scope notes

The original plan listed `duplicate`, `corrupt`, `reorder`,
`loss_correlation`, `delay_correlation` for the same flip. None of
those exist on the declarative `QdiscBuilder` — they live on the
imperative `NetemConfig`, which already takes `Percent`. The
declarative netem model carries only `delay_us`, `jitter_us`,
`loss_percent`, `limit`. Extension of the declarative model to
match the imperative one is a 0.21 candidate.

## Plan 230 — `ChainName` newtype + `Verdict::JumpTo`/`GotoTo` variants

### Added

- `nlink::netlink::nftables::types::ChainName` newtype.
  Constructor `ChainName::new(impl Into<String>) -> Result<Self>`
  validates: non-empty, no interior NUL, ≤ 255 bytes
  (`NFT_NAME_MAXLEN - 1`). Implements `Display`, `AsRef<str>`,
  `as_str`, `TryFrom<&str>`, `TryFrom<String>`.
- `Verdict::JumpTo(ChainName)` and `Verdict::GotoTo(ChainName)` — new
  `#[non_exhaustive]` enum variants. Emit identical wire bytes to the
  deprecated `Jump(String)` / `Goto(String)` (the typed boundary just
  surfaces invalid names at construction instead of as a late kernel
  rejection at apply time).
- `RuleBuilder::jump_to(ChainName)` and `goto_to(ChainName)` —
  convenience constructors for the typed variants.

### Deprecated

- `Verdict::Jump(String)` — use `Verdict::JumpTo(ChainName::new(...)?)`.
- `Verdict::Goto(String)` — use `Verdict::GotoTo(ChainName::new(...)?)`.

The bare-`String` form lets interior NULs and overlong names through
to a kernel rejection at apply time; the typed sibling validates at
construction.

### Internal notes

`RuleBuilder::jump(&str)` and `goto(&str)` keep their existing
infallible signature — they now route to `Verdict::JumpTo`/`GotoTo`
when `ChainName::new` succeeds and fall back to the deprecated
`Verdict::Jump`/`Goto` variant on rejection (so the public signature
stays infallible while still emitting wire bytes for any input).
Wire-format parity is asserted by Plan 230's expr unit tests.

## Plan 231 — `RuleMessage` per-field accessors

### Added

- `RuleMessage::family_typed() -> AddressFamily` — Plan 227 typed
  sibling of the existing `family() -> u8`.
- `RuleMessage::priority()`, `source()`, `destination()`, `iifname()`,
  `oifname()`, `fwmark()`, `fwmask()`, `table()`, `goto()`, `flow()`,
  `tun_id()`, `suppress_ifgroup()`, `suppress_prefixlen()`, `l3mdev()`,
  `uid_range()`, `protocol()`, `ip_proto()`, `sport_range()`,
  `dport_range()` — accessor methods mirroring every load-bearing
  field. The fields stay `pub` (0.20.1 is a patch release; flipping
  visibility is breaking). New code should prefer the accessors.

### Deprecated

(none — field-read deprecation isn't supported at the language level
and the accessor pattern doesn't require it; users migrate to accessors
organically.)

### Sibling notes (deferred)

The original plan §4.2 audit listed `BridgeVlanMessage`, `FdbMessage`,
`MplsRouteMessage`, `NexthopMessage` as candidates for the same
sweep. The standard messages (`LinkMessage`, `AddressMessage`,
`RouteMessage`, `NeighborMessage`, `TcMessage`) already follow the
accessor convention. The four candidates are flagged for individual
review in a follow-on patch — same accessor-only-no-field-visibility
discipline applies, but the cost-benefit per struct varies.

`scripts/audit-message-accessor-convention.sh` (plan §6.4) is also
deferred — it would fail loudly on the `pub` fields the additive
adaptation deliberately preserves.
