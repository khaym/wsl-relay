use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::AbortHandle;
use tokio::time::Instant;

use super::backend::{POWER_REQUEST_REASON, PowerBackend, PowerRequestHandle};

/// Default inhibit duration when the client does not specify one.
/// Long enough to bridge gaps between client heartbeats (e.g. tool-less
/// generation in Claude Code sends no hook events).
pub const DEFAULT_TTL_SECS: u64 = 600;

/// Upper bound for client-specified TTLs. Caps how long an inhibit can
/// outlive a crashed client; oversized requests are clamped, not rejected.
pub const MAX_TTL_SECS: u64 = 3600;

struct InhibitSlot {
    handle: Box<dyn PowerRequestHandle>,
    expires_at: Instant,
}

struct State {
    slot: Option<InhibitSlot>,
    // Expiry timer for the current slot; aborted and replaced on renew,
    // aborted on release, so at most one timer task is alive at a time.
    timer: Option<AbortHandle>,
}

/// Owns the single system-wide inhibit slot: creates the OS power request on
/// first acquire, extends the deadline on renewals, and guarantees release via
/// an expiry timer even if the client never calls back.
///
/// Backend calls happen inline (power request syscalls are non-blocking),
/// so no `spawn_blocking` is needed here, unlike the notify/clipboard routes.
pub struct PowerInhibitManager {
    backend: Arc<dyn PowerBackend>,
    state: Mutex<State>,
}

impl PowerInhibitManager {
    pub fn new(backend: Arc<dyn PowerBackend>) -> Arc<Self> {
        Arc::new(Self {
            backend,
            state: Mutex::new(State {
                slot: None,
                timer: None,
            }),
        })
    }

    /// Start inhibiting, or extend the deadline if already active (idempotent).
    /// The latest call wins: the deadline is always reset to now + ttl.
    /// Returns the effective TTL after clamping to [`MAX_TTL_SECS`].
    /// Must be called within a tokio runtime (spawns the expiry timer).
    pub fn acquire_or_renew(self: &Arc<Self>, ttl: Duration) -> anyhow::Result<Duration> {
        let ttl = ttl.min(Duration::from_secs(MAX_TTL_SECS));
        let expires_at = Instant::now() + ttl;

        let mut state = self.state.lock().unwrap();
        if state.slot.is_none() {
            let handle = self.backend.create_request(POWER_REQUEST_REASON)?;
            state.slot = Some(InhibitSlot { handle, expires_at });
        } else if let Some(slot) = state.slot.as_mut() {
            slot.expires_at = expires_at;
        }

        if let Some(timer) = state.timer.take() {
            timer.abort();
        }
        let manager = Arc::clone(self);
        state.timer = Some(
            tokio::spawn(async move {
                tokio::time::sleep_until(expires_at).await;
                manager.expire();
            })
            .abort_handle(),
        );

        Ok(ttl)
    }

    /// Release the inhibit immediately. No-op when not active.
    /// A failure to clear the OS request is logged, not surfaced: the slot is
    /// gone either way, and dropping the handle releases the request.
    pub fn release(&self) {
        let slot = {
            let mut state = self.state.lock().unwrap();
            if let Some(timer) = state.timer.take() {
                timer.abort();
            }
            state.slot.take()
        };
        if let Some(mut slot) = slot
            && let Err(e) = slot.handle.clear()
        {
            tracing::error!("Failed to clear released power request: {e}");
        }
    }

    /// Time until auto-release, or `None` when not inhibiting.
    /// A slot past its deadline (expiry timer not yet run) counts as inactive.
    pub fn remaining(&self) -> Option<Duration> {
        let state = self.state.lock().unwrap();
        state
            .slot
            .as_ref()
            .map(|slot| slot.expires_at.saturating_duration_since(Instant::now()))
            .filter(|remaining| !remaining.is_zero())
    }

    fn expire(&self) {
        let slot = {
            let mut state = self.state.lock().unwrap();
            // A renew may have moved the deadline after this timer was already
            // running (abort can't stop a woken task); only clear if the
            // current slot really is past due.
            match state.slot.as_ref() {
                Some(slot) if Instant::now() >= slot.expires_at => {
                    state.timer = None;
                    state.slot.take()
                }
                _ => return,
            }
        };
        if let Some(mut slot) = slot
            && let Err(e) = slot.handle.clear()
        {
            tracing::error!("Failed to clear expired power request: {e}");
        }
    }
}
