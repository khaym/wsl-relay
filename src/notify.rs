use serde::Deserialize;

/// Application User Model ID for wsl-relay.
/// Used to identify the app in Windows toast notifications and Start menu.
pub const WSLRELAY_AUMID: &str = "WslRelay.HostAgent";

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NotifyIcon {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotifyRequest {
    pub title: String,
    pub body: String,
    #[serde(default = "default_icon")]
    pub icon: NotifyIcon,
}

fn default_icon() -> NotifyIcon {
    NotifyIcon::Info
}

pub trait NotificationBackend: Send + Sync {
    fn notify(&self, req: &NotifyRequest) -> anyhow::Result<()>;
}

pub struct StubNotifier;

impl NotificationBackend for StubNotifier {
    fn notify(&self, _req: &NotifyRequest) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Escape XML special characters to prevent injection.
pub fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "windows")]
pub struct WindowsNotifier;

#[cfg(target_os = "windows")]
impl NotificationBackend for WindowsNotifier {
    fn notify(&self, req: &NotifyRequest) -> anyhow::Result<()> {
        use windows::Data::Xml::Dom::XmlDocument;
        use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
        use windows::core::HSTRING;

        let title = escape_xml(&req.title);
        let body = escape_xml(&req.body);

        let template = format!(
            r#"<toast>
                <visual>
                    <binding template="ToastGeneric">
                        <text>{title}</text>
                        <text>{body}</text>
                    </binding>
                </visual>
            </toast>"#
        );

        let xml = XmlDocument::new()?;
        xml.LoadXml(&HSTRING::from(&template))?;
        let toast = ToastNotification::CreateToastNotification(&xml)?;
        let notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(WSLRELAY_AUMID))?;
        notifier.Show(&toast)?;
        Ok(())
    }
}

/// Register the app's AUMID in the Windows registry so toast notifications
/// display with the app's own identity instead of PowerShell's.
/// Writes to HKCU (no elevation required).
#[cfg(target_os = "windows")]
pub fn register_aumid() -> anyhow::Result<()> {
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, KEY_WRITE, REG_SZ, RegCreateKeyExW, RegSetValueExW,
    };
    use windows::core::HSTRING;

    let subkey = HSTRING::from(format!(
        "Software\\Classes\\AppUserModelId\\{}",
        WSLRELAY_AUMID
    ));
    let display_name = HSTRING::from("WSL Relay");

    unsafe {
        let mut hkey = Default::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &subkey,
            0,
            None,
            Default::default(),
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )?;

        // C4 fix: include null terminator for REG_SZ
        let name = HSTRING::from("DisplayName");
        let wide_with_null: Vec<u16> = "WSL Relay".encode_utf16().chain(std::iter::once(0)).collect();
        let byte_slice = std::slice::from_raw_parts(
            wide_with_null.as_ptr() as *const u8,
            wide_with_null.len() * std::mem::size_of::<u16>(),
        );
        RegSetValueExW(hkey, &name, 0, REG_SZ, Some(byte_slice))?;

        // S1 fix: close registry key handle
        use windows::Win32::System::Registry::RegCloseKey;
        RegCloseKey(hkey)?;
    }

    Ok(())
}
