// SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
// SPDX-License-Identifier: Apache-2.0

//! Watcher constants.

use std::num::NonZeroUsize;
use std::time::Duration;

/// Default debounce duration - wait this long after last write before emitting event.
pub const DEFAULT_DEBOUNCE_DURATION: Duration = Duration::from_millis(300);

/// Inotify read buffer size in bytes.
pub(super) const INOTIFY_BUFFER_SIZE: usize = 16_384;

/// Minimum poll interval to avoid busy-looping.
pub(super) const MIN_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Default maximum pending files before forcing early processing.
pub(super) const DEFAULT_MAX_PENDING: usize = 16_384;

/// Maximum total items across all queues before triggering overflow.
pub(super) const MAX_TOTAL_QUEUED: usize = 65_536;

/// Overflow backoff: base delay before rescan.
pub(super) const OVERFLOW_BACKOFF_BASE: Duration = Duration::from_secs(2);

/// Overflow backoff: maximum delay (cap for exponential growth).
pub(super) const OVERFLOW_BACKOFF_MAX: Duration = Duration::from_secs(60);

/// Overflow backoff: reset count after this period without overflow.
pub(super) const OVERFLOW_RESET_AFTER: Duration = Duration::from_secs(300);

/// Minimum capacity for small collections after shrinking.
pub(super) const SHRINK_MIN_CAPACITY: usize = 128;

/// Timeout for pending `MOVED_FROM` events waiting for matching `MOVED_TO`.
pub(super) const MOVE_COOKIE_TIMEOUT: Duration = Duration::from_secs(2);

/// Maximum entries in the loop prevention LRU cache.
pub(super) const SKIP_CACHE_SIZE: NonZeroUsize = match NonZeroUsize::new(16_384) {
    Some(n) => n,
    None => panic!("SKIP_CACHE_SIZE must be non-zero"),
};
