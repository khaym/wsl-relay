mod backend;
mod manager;

#[cfg(target_os = "windows")]
pub use backend::WindowsPowerBackend;
pub use backend::{PowerBackend, PowerRequestHandle, StubPowerBackend};
pub use manager::{DEFAULT_TTL_SECS, MAX_TTL_SECS, PowerInhibitManager};
