//! ENOBUFS-resync helper types.
//!
//! When a multicast subscriber falls behind the kernel's event
//! production rate, the kernel drops events and returns `ENOBUFS`
//! on the next `recvmsg`. The subscriber's view of state is now
//! incomplete. The correct recovery (per kernel maintainers'
//! guidance) is:
//!
//! 1. Re-dump current state via the matching `get_*` method.
//! 2. Resume the multicast stream from where the read left off.
//!
//! Downstream consumers of this pattern keep reinventing it, often
//! badly (the well-known
//! [Cilium issue #40280](https://github.com/cilium/cilium/issues/40280)
//! is the same gap in Go). This module ships the **types** that
//! make the pattern explicit:
//!
//! - [`ResyncedEvent<T>`] — sum type yielded by a resync-aware
//!   consumer: `Event(T)` for normal events, `Resynced(T)` for
//!   replayed items, `Marker(...)` for state-machine boundaries.
//! - [`ResyncMarker`] — `ResyncStart` and `ResyncEnd` boundaries
//!   so consumers can coordinate state-rebuild logic with the
//!   replay window.
//!
//! See `docs/recipes/events-with-resync.md` for the canonical
//! event-loop pattern using these types. A pre-baked Stream
//! wrapper that drives the state machine internally (planned in
//! Plan 151 §4.2) is a follow-up — the design needs more soak
//! before it's locked in.
//!
//! # Example loop
//!
//! ```ignore
//! use nlink::netlink::resync::{ResyncedEvent, ResyncMarker};
//! use tokio_stream::StreamExt;
//!
//! # async fn run(
//! #     mut events: nlink::netlink::stream::EventSubscription<'_, nlink::Route>,
//! #     dump_conn: &nlink::Connection<nlink::Route>,
//! #     mut handle: impl FnMut(ResyncedEvent<nlink::netlink::messages::LinkMessage>),
//! # ) -> nlink::Result<()> {
//! while let Some(item) = events.next().await {
//!     match item {
//!         Ok(ev) => handle(ResyncedEvent::Event(ev)),
//!         Err(e) if e.is_no_buffer_space() => {
//!             handle(ResyncedEvent::Marker(ResyncMarker::ResyncStart));
//!             for link in dump_conn.get_links().await? {
//!                 handle(ResyncedEvent::Resynced(link));
//!             }
//!             handle(ResyncedEvent::Marker(ResyncMarker::ResyncEnd));
//!         }
//!         Err(other) => return Err(other),
//!     }
//! }
//! # Ok(())
//! # }
//! ```

/// Boundary markers emitted around a resync window so consumers
/// can coordinate state-rebuild logic with the replay.
///
/// `ResyncStart` is the cue to invalidate any incremental state
/// the consumer has been accumulating from `Event(T)`s (it's now
/// stale).
///
/// `ResyncEnd` is the cue that the replay is complete — the
/// consumer's state now reflects current kernel state, and
/// subsequent `Event(T)`s are real-time deltas again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResyncMarker {
    /// Resync is starting. The next items will be
    /// [`ResyncedEvent::Resynced`] until [`Self::ResyncEnd`].
    ResyncStart,
    /// Resync is complete. Subsequent items resume as
    /// [`ResyncedEvent::Event`].
    ResyncEnd,
}

/// A stream item produced by a resync-aware event consumer.
///
/// Distinguishes multicast event deltas (`Event`) from
/// post-overflow state replay (`Resynced`), with explicit
/// boundary markers so the consumer's state-rebuild logic can
/// trigger at the right moment.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResyncedEvent<T> {
    /// A real-time multicast event from the kernel.
    Event(T),
    /// A state-snapshot item from the post-`ENOBUFS` redump.
    Resynced(T),
    /// A boundary marker. See [`ResyncMarker`].
    Marker(ResyncMarker),
}

impl<T> ResyncedEvent<T> {
    /// Convenience: is this a `Marker(ResyncStart)`?
    pub fn is_resync_start(&self) -> bool {
        matches!(self, Self::Marker(ResyncMarker::ResyncStart))
    }

    /// Convenience: is this a `Marker(ResyncEnd)`?
    pub fn is_resync_end(&self) -> bool {
        matches!(self, Self::Marker(ResyncMarker::ResyncEnd))
    }

    /// Extract the inner `T`, regardless of whether it arrived as
    /// a real-time event or a replay item. Returns `None` for
    /// marker variants (callers usually want to handle markers
    /// separately).
    pub fn into_inner(self) -> Option<T> {
        match self {
            Self::Event(t) | Self::Resynced(t) => Some(t),
            Self::Marker(_) => None,
        }
    }

    /// Borrow the inner `T`. `None` for markers.
    pub fn as_inner(&self) -> Option<&T> {
        match self {
            Self::Event(t) | Self::Resynced(t) => Some(t),
            Self::Marker(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_predicates() {
        let start: ResyncedEvent<u32> = ResyncedEvent::Marker(ResyncMarker::ResyncStart);
        let end: ResyncedEvent<u32> = ResyncedEvent::Marker(ResyncMarker::ResyncEnd);
        let event = ResyncedEvent::Event(42u32);
        let resynced = ResyncedEvent::Resynced(7u32);

        assert!(start.is_resync_start());
        assert!(!start.is_resync_end());
        assert!(end.is_resync_end());
        assert!(!end.is_resync_start());
        assert!(!event.is_resync_start());
        assert!(!resynced.is_resync_end());
    }

    #[test]
    fn inner_extraction_skips_markers() {
        let start: ResyncedEvent<u32> = ResyncedEvent::Marker(ResyncMarker::ResyncStart);
        let event = ResyncedEvent::Event(42u32);
        let resynced = ResyncedEvent::Resynced(7u32);

        assert_eq!(start.clone().into_inner(), None);
        assert_eq!(event.clone().into_inner(), Some(42));
        assert_eq!(resynced.clone().into_inner(), Some(7));

        assert_eq!(start.as_inner(), None);
        assert_eq!(event.as_inner(), Some(&42));
        assert_eq!(resynced.as_inner(), Some(&7));
    }
}
