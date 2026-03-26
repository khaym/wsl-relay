pub trait AutostartBackend: Send + Sync {
    fn enable(&self) -> anyhow::Result<()>;
    fn disable(&self) -> anyhow::Result<()>;
    fn is_enabled(&self) -> anyhow::Result<bool>;
}

pub struct StubAutostart;

impl AutostartBackend for StubAutostart {
    fn enable(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn disable(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn is_enabled(&self) -> anyhow::Result<bool> {
        Ok(false)
    }
}

#[cfg(target_os = "windows")]
const REGISTRY_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const REGISTRY_VALUE_NAME: &str = "WslRelay";

#[cfg(target_os = "windows")]
pub struct WindowsAutostart;

#[cfg(target_os = "windows")]
impl AutostartBackend for WindowsAutostart {
    fn enable(&self) -> anyhow::Result<()> {
        use anyhow::Context;
        use windows::Win32::System::Registry::{
            HKEY_CURRENT_USER, KEY_WRITE, REG_SZ, RegCloseKey, RegCreateKeyExW, RegSetValueExW,
        };
        use windows::core::HSTRING;

        let exe_path = std::env::current_exe().context("Failed to get current exe path")?;
        let exe_str = exe_path
            .to_str()
            .context("Exe path contains invalid Unicode")?;
        // Quote the path to handle spaces in Windows paths (e.g. "C:\Program Files\...")
        let exe_quoted = format!("\"{}\"", exe_str);

        let subkey = HSTRING::from(REGISTRY_SUBKEY);
        let value_name = HSTRING::from(REGISTRY_VALUE_NAME);

        unsafe {
            let mut hkey = Default::default();
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                &subkey,
                None,
                None,
                Default::default(),
                KEY_WRITE,
                None,
                &mut hkey,
                None,
            )
            .context("Failed to open/create registry key")?;

            let wide_with_null: Vec<u16> = exe_quoted
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let byte_slice = std::slice::from_raw_parts(
                wide_with_null.as_ptr() as *const u8,
                wide_with_null.len() * std::mem::size_of::<u16>(),
            );
            let result = RegSetValueExW(hkey, &value_name, None, REG_SZ, Some(byte_slice));
            RegCloseKey(hkey);
            result.context("Failed to set registry value")?;
        }

        Ok(())
    }

    fn disable(&self) -> anyhow::Result<()> {
        use anyhow::Context;
        use windows::Win32::System::Registry::{
            HKEY_CURRENT_USER, KEY_WRITE, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
        };
        use windows::core::HSTRING;

        let subkey = HSTRING::from(REGISTRY_SUBKEY);
        let value_name = HSTRING::from(REGISTRY_VALUE_NAME);

        unsafe {
            let mut hkey = Default::default();
            let open_result = RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, None, KEY_WRITE, &mut hkey);
            if open_result.is_err() {
                // Key doesn't exist, nothing to disable
                return Ok(());
            }

            let result = RegDeleteValueW(hkey, &value_name);
            RegCloseKey(hkey);
            // ERROR_FILE_NOT_FOUND means the value didn't exist — not an error
            if result.is_err() {
                use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
                let code = windows::core::Error::from(result).code();
                if code != ERROR_FILE_NOT_FOUND.to_hresult() {
                    anyhow::bail!("Failed to delete registry value: {}", code);
                }
            }
        }

        Ok(())
    }

    fn is_enabled(&self) -> anyhow::Result<bool> {
        use windows::Win32::System::Registry::{
            HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
        };
        use windows::core::HSTRING;

        let subkey = HSTRING::from(REGISTRY_SUBKEY);
        let value_name = HSTRING::from(REGISTRY_VALUE_NAME);

        unsafe {
            let mut hkey = Default::default();
            let open_result = RegOpenKeyExW(HKEY_CURRENT_USER, &subkey, None, KEY_READ, &mut hkey);
            if open_result.is_err() {
                return Ok(false);
            }

            let result = RegQueryValueExW(hkey, &value_name, None, None, None, None);
            RegCloseKey(hkey);
            Ok(result.is_ok())
        }
    }
}
