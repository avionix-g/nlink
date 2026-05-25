---
to: nlink maintainers
from: 0.16 cycle Plan 168 orphan-example forensic (carried into 0.17)
subject: `Bottleneck::score: f64` — normalized severity score for the diagnostics layer
status: proposed for 0.17 — needs sign-off on the combining formula
target version: 0.17.0
parent: 177-0.17-master-plan.md
created: 2026-05-25
---

# Plan 169 — `Bottleneck::score` design call

## 1. Why this plan exists

The 0.16 orphan-example forensic pass found 5 lib-side gaps
that the diagnostics layer dropped. Four were trivially
additive and landed in 0.16 (RouteInfo source + dev_name,
InterfaceDiag predicates, Srv6LocalRoute table getter). The
fifth — `Bottleneck::score: f64` — is a design call that
needed maintainer input on the combining formula, so it
shipped as "deferred to 0.17."

This plan is the remainder of that work.

## 2. The orphan's expectation

`bottleneck.score: f64` — a normalized 0.0..=1.0 number
suitable for "alert if > 0.5" monitoring patterns.

`Bottleneck` today (`diagnostics.rs:344`) has:
- `drop_rate: f64` (already 0.0..=1.0)
- `total_drops: u64`
- `current_rate: u64`
- `recommendation: String`

No single "how severe?" metric.

## 3. Design choices

| Option | Description | Verdict |
|---|---|---|
| **A. Don't add it.** | `drop_rate` already 0..1; users wanting more can combine fields themselves. | Cheapest but the orphan's evidence says users WANT this. |
| **B. Alias for drop_rate.** | `score()` returns `drop_rate`. Trivial. | Lossy vs. combining signals; misleading name. |
| **C. Composite metric.** | Combine drop_rate + backlog pressure + error rate with documented weights. | Committed formula; users who need different weighting compute their own from the underlying fields. |
| **D. `Severity` enum.** | Quantized (Info/Warn/Error/Critical), similar to the existing `Severity` enum. | Less granular than f64; harder to use in numeric alerts. |

**Recommendation: option C** with the formula in §4.

## 4. Proposed implementation

```rust
impl Bottleneck {
    /// Normalized severity score in 0.0..=1.0.
    ///
    /// Combines three signals with documented weights:
    /// - `drop_rate` (weight 0.6) — the most direct measure of
    ///   "packets are being lost."
    /// - Backlog pressure (weight 0.3) — `min(backlog / 1MB,
    ///   1.0)`, where `backlog` is `BottleneckType`-specific
    ///   (qdisc backlog in bytes for `QdiscDrops`/`BufferFull`;
    ///   0 for the others).
    /// - Error rate (weight 0.1) — `min(total_errors as f64 /
    ///   total_packets as f64, 1.0)` for `HardwareErrors`; 0
    ///   for the others.
    ///
    /// Saturates at 1.0. Higher = worse.
    ///
    /// **The weights are documented rather than configurable.**
    /// For use cases that need different weighting (e.g., a
    /// latency-RT workload that treats any drop as critical),
    /// compute the score yourself from `self.drop_rate`,
    /// `self.total_drops`, `self.current_rate`, etc. The lib
    /// commits to this formula across 0.17.x.
    pub fn score(&self) -> f64 {
        let drop_component = self.drop_rate * 0.6;

        let backlog_pressure = match self.bottleneck_type {
            BottleneckType::QdiscDrops | BottleneckType::BufferFull => {
                // current_rate doubles as the backlog proxy in
                // the existing field set; normalize against a
                // 1 MB rough ceiling.
                (self.current_rate as f64 / 1_048_576.0).min(1.0)
            }
            _ => 0.0,
        };
        let backlog_component = backlog_pressure * 0.3;

        let error_component = match self.bottleneck_type {
            BottleneckType::HardwareErrors | BottleneckType::InterfaceDrops => {
                // total_drops here is a proxy for "errors
                // observed"; normalize against current_rate to
                // get a per-packet error rate.
                if self.current_rate > 0 {
                    (self.total_drops as f64 / self.current_rate as f64).min(1.0) * 0.1
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };

        (drop_component + backlog_component + error_component).min(1.0)
    }
}
```

## 5. Acceptance criteria

- [ ] `Bottleneck::score() -> f64` implemented per §4.
- [ ] Documented formula in rustdoc + recipe note on when to
      override.
- [ ] Unit tests:
    - Empty `Bottleneck` (all zeros) → 0.0.
    - Pure drop rate (1.0, no backlog, no errors) → 0.6.
    - Pure backlog (1MB backlog, no drops, no errors) on a
      `BufferFull` → 0.3.
    - Composite (0.5 drop_rate + 0.5MB backlog on QdiscDrops
      + zero errors) → 0.45.
    - Saturates at 1.0 in extreme cases.
- [ ] CHANGELOG entry under `### Added`.

## 6. Effort estimate

| Phase | Effort |
|---|---|
| 1 impl + rustdoc | ~30 min |
| 2 unit tests | ~30 min |
| 3 CHANGELOG | ~10 min |
| **Total** | **~1 h** |

## 7. Risks

- **Weights are wrong for some workloads** — by design; the
  rustdoc tells users to override. Mitigation: document in the
  CHANGELOG that the formula is committed across 0.17.x but
  open to revision in 0.18 if real-world signal indicates.
- **`current_rate` overload** — the field today is "current
  bytes/sec," not "backlog bytes." Using it as the backlog
  proxy is a coarse approximation. If `Bottleneck` later gains
  a dedicated `backlog_bytes: u64` field, update §4's formula
  to use it.

## 8. Out-of-scope follow-ups

- **Configurable weights** (`BottleneckScoreConfig`): nice but
  adds API surface for a minority use case. Plan 178+ if real
  demand surfaces.
- **`Severity` quantization helper** (`score() -> Severity`):
  trivial wrapper on top of `score() -> f64`. Defer until a
  caller actually wants it.

End of plan.
