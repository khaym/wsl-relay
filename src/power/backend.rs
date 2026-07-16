/// Reason string attached to the OS power request.
/// Visible on the host via `powercfg /requests` while an inhibit is active.
pub const POWER_REQUEST_REASON: &str = "WSL Relay: client task in progress";

/// A live OS-level power request. Dropping without `clear` must not leak
/// the request (Windows releases it when the handle is closed / process exits).
pub trait PowerRequestHandle: Send {
    fn clear(&mut self) -> anyhow::Result<()>;
}

pub trait PowerBackend: Send + Sync {
    fn create_request(&self, reason: &str) -> anyhow::Result<Box<dyn PowerRequestHandle>>;
}

pub struct StubPowerBackend;

struct StubPowerHandle;

impl PowerRequestHandle for StubPowerHandle {
    fn clear(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

impl PowerBackend for StubPowerBackend {
    fn create_request(&self, _reason: &str) -> anyhow::Result<Box<dyn PowerRequestHandle>> {
        Ok(Box::new(StubPowerHandle))
    }
}

#[cfg(target_os = "windows")]
pub struct WindowsPowerBackend;

#[cfg(target_os = "windows")]
struct WindowsPowerHandle {
    handle: windows::Win32::Foundation::HANDLE,
    cleared: bool,
}

// SAFETY: a power request handle is a process-wide kernel object handle;
// it is not tied to the thread that created it.
#[cfg(target_os = "windows")]
unsafe impl Send for WindowsPowerHandle {}

#[cfg(target_os = "windows")]
impl PowerRequestHandle for WindowsPowerHandle {
    fn clear(&mut self) -> anyhow::Result<()> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Power::{PowerClearRequest, PowerRequestSystemRequired};

        if self.cleared {
            return Ok(());
        }
        unsafe {
            PowerClearRequest(self.handle, PowerRequestSystemRequired)?;
            CloseHandle(self.handle)?;
        }
        self.cleared = true;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsPowerHandle {
    fn drop(&mut self) {
        use windows::Win32::Foundation::CloseHandle;

        if !self.cleared {
            // Closing the handle also releases the outstanding power request.
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl PowerBackend for WindowsPowerBackend {
    fn create_request(&self, reason: &str) -> anyhow::Result<Box<dyn PowerRequestHandle>> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Power::{
            PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest,
        };
        use windows::Win32::System::SystemServices::DIAGNOSTIC_REASON_VERSION;
        use windows::Win32::System::Threading::{
            POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
        };
        use windows::core::PWSTR;

        let mut reason_wide: Vec<u16> = reason.encode_utf16().chain(std::iter::once(0)).collect();
        let context = REASON_CONTEXT {
            Version: DIAGNOSTIC_REASON_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: PWSTR(reason_wide.as_mut_ptr()),
            },
        };

        unsafe {
            let handle = PowerCreateRequest(&context)?;
            if let Err(e) = PowerSetRequest(handle, PowerRequestSystemRequired) {
                let _ = CloseHandle(handle);
                return Err(e.into());
            }
            Ok(Box::new(WindowsPowerHandle {
                handle,
                cleared: false,
            }))
        }
    }
}
