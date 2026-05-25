---
to: nlink maintainers
from: 0.16 cycle Plan 168 forensic (2026-05-25)
subject: `From<AddressParseError>` + `From<RouteParseError>` impls for `nlink::Error`
status: proposed for 0.17 — pure additive, ~30 min, do-it-now scope
target version: 0.17.0
parent: 177-0.17-master-plan.md
source: Plan 168 Phase 3 — `config/declarative.rs` rewrite hit this papercut
created: 2026-05-25
---

# Plan 173 — parse-error `From` impls

## 1. The papercut

`NetworkConfig::address(dev, addr)` and `NetworkConfig::route(dst, f)`
return `Result<Self, AddressParseError>` and
`Result<Self, RouteParseError>` respectively. Those error types
do **not** impl `From<X> for nlink::Error`. So every caller
chaining them with `?` against an outer `nlink::Result<()>` has
to manually map:

```rust
let cfg = NetworkConfig::new()
    .link("d0", |l| l.dummy().up())
    .address("d0", "10.99.0.1/24")
        .map_err(|e| nlink::Error::InvalidMessage(e.to_string()))?
    .route("10.99.1.0/24", |r| r.via("10.99.0.254"))
        .map_err(|e| nlink::Error::InvalidMessage(e.to_string()))?;
```

vs. the symmetric ideal:

```rust
let cfg = NetworkConfig::new()
    .link("d0", |l| l.dummy().up())
    .address("d0", "10.99.0.1/24")?           // From impl converts
    .route("10.99.1.0/24", |r| r.via("10.99.0.254"))?;
```

The Plan 168 Phase 3 rewrite of `examples/config/declarative.rs`
hit this and had to use the `map_err` shape — embarrassing for
an example file demonstrating the API.

## 2. The fix

Add two trait impls:

```rust
// crates/nlink/src/netlink/error.rs

impl From<crate::netlink::config::types::AddressParseError> for Error {
    fn from(e: crate::netlink::config::types::AddressParseError) -> Self {
        Error::InvalidMessage(e.to_string())
    }
}

impl From<crate::netlink::config::types::RouteParseError> for Error {
    fn from(e: crate::netlink::config::types::RouteParseError) -> Self {
        Error::InvalidMessage(e.to_string())
    }
}
```

Or — since `Error` already uses `#[from]` via `thiserror` for
its other inner-error variants — add the two as enum variants:

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    // ... existing variants ...

    #[error("address parse: {0}")]
    AddressParse(#[from] AddressParseError),

    #[error("route parse: {0}")]
    RouteParse(#[from] RouteParseError),
}
```

The second shape is more idiomatic + preserves the typed
source, but adds two enum variants. Since `Error` is already
`#[non_exhaustive]` (per Plan 163 lockdown), the variant
addition is non-breaking.

**Recommendation**: enum variants — typed sources are better
debug UX than `InvalidMessage(stringified)`.

## 3. Apply at every caller

The `examples/config/declarative.rs` `map_err()` workaround can
go away:

```rust
.address("declarative_dummy0", "10.99.0.1/24")?   // ← no map_err
.route("10.99.1.0/24", |r| r.via("10.99.0.254"))?
```

Grep for other affected files:

```bash
grep -rn "AddressParseError\|RouteParseError" crates/nlink/ docs/ \
    --exclude-dir=target | grep -v "^crates/nlink/src/netlink/error.rs"
```

Likely affected: `examples/config/declarative.rs` (we fixed
this one's instance with `map_err`); maybe recipe docs.

## 4. Acceptance criteria

- [ ] `nlink::Error` gains `AddressParse(AddressParseError)`
      and `RouteParse(RouteParseError)` variants with
      `#[from]` derived impls.
- [ ] `examples/config/declarative.rs` simplifies its 2-3
      `map_err()` calls to plain `?`.
- [ ] Any recipe doc with the old `map_err` workaround pattern
      gets updated.
- [ ] `cargo build -p nlink --all-targets` clean.
- [ ] CHANGELOG entry under `### Added` (additive).

## 5. Effort estimate

| Phase | Effort |
|---|---|
| 1 add enum variants in `error.rs` | ~10 min |
| 2 simplify `examples/config/declarative.rs` | ~5 min |
| 3 grep for + fix any other `map_err` workarounds | ~10 min |
| 4 CHANGELOG | ~5 min |
| **Total** | **~30 min** |

## 6. Risks

- **None substantive.** Adding enum variants to a
  `#[non_exhaustive]` enum is additive. Callers that pattern-
  matched `Error` exhaustively already have a `_ => {}` arm
  (or get a compile error they should fix).
- **The `e.to_string()` shape may be incompatible with the
  enum-variant shape**: if the parse-error types' Display
  impls aren't great, the new `Error::AddressParse(e)` printed
  via `Display` might read worse than the previous
  `InvalidMessage("…")`. Spot-check at write time.

## 7. Out-of-scope follow-ups

- **A general "do every type that returns Result<_, X>
  expose an `Into<nlink::Error>`?" audit**: bigger scope.
  These two were the only ones surfaced by 0.16 work. Plan
  179 territory if more cases emerge.

End of plan.
