use serde::Deserialize;

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
        // Use PowerShell's registered App ID as a workaround.
        // This AUMID is consistent across all Windows 10/11 installations.
        let app_id = "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\
                       \\WindowsPowerShell\\v1.0\\powershell.exe";
        let notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id))?;
        notifier.Show(&toast)?;
        Ok(())
    }
}
