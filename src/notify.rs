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
        use windows::core::HSTRING;
        use windows::Data::Xml::Dom::XmlDocument;
        use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

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
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from("wsl-relay"))?;
        notifier.Show(&toast)?;
        Ok(())
    }
}
