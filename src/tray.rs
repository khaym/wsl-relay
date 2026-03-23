use std::sync::mpsc::SyncSender;

/// Embedded tray icon (16x16 ICO).
pub const TRAY_ICON_BYTES: &[u8] = include_bytes!("../assets/tray.ico");

pub trait TrayBackend: Send + Sync {
    fn run(&self, quit_tx: SyncSender<()>) -> anyhow::Result<()>;
}

/// Stub implementation for non-Windows platforms and testing.
/// Blocks the thread indefinitely, simulating the behavior of a real tray
/// message loop that blocks until "Quit" is selected.
pub struct StubTray;

impl TrayBackend for StubTray {
    fn run(&self, _quit_tx: SyncSender<()>) -> anyhow::Result<()> {
        // W1 fix: loop to handle spurious wakeups from thread::park().
        // We never send on quit_tx — shutdown is driven by Ctrl+C / signal.
        loop {
            std::thread::park();
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use std::sync::mpsc::SyncSender;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{
        NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CW_USEDEFAULT, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
        DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, MF_STRING, MSG,
        PostQuitMessage, RegisterClassW, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
        TrackPopupMenu, WM_APP, WM_COMMAND, WM_DESTROY, WM_RBUTTONUP, WNDCLASSW,
        WS_OVERLAPPEDWINDOW,
    };
    use windows::core::w;

    const WM_TRAYICON: u32 = WM_APP + 1;
    const IDM_QUIT: u16 = 1001;

    pub struct WindowsTray;

    impl TrayBackend for WindowsTray {
        fn run(&self, quit_tx: SyncSender<()>) -> anyhow::Result<()> {
            unsafe { run_tray_loop(quit_tx) }
        }
    }

    unsafe fn run_tray_loop(quit_tx: SyncSender<()>) -> anyhow::Result<()> {
        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("WslRelayTrayClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(tray_wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };

        // C2 fix: handle RegisterClassW failure (allow ERROR_CLASS_ALREADY_EXISTS)
        let atom = RegisterClassW(&wc);
        if atom == 0 {
            let err = windows::core::Error::from_win32();
            // ERROR_CLASS_ALREADY_EXISTS = 1410
            if err.code().0 as u32 != 0x80070582 {
                return Err(anyhow::anyhow!("RegisterClassW failed: {}", err));
            }
        }

        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            w!("WSL Relay Tray"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            Some(hinstance.into()),
            None,
        )?;

        let icon = load_embedded_icon();

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: icon.unwrap_or_default(),
            ..Default::default()
        };

        // Set tooltip
        let tip = "WSL Relay";
        let tip_wide: Vec<u16> = tip.encode_utf16().collect();
        let len = tip_wide.len().min(nid.szTip.len() - 1);
        nid.szTip[..len].copy_from_slice(&tip_wide[..len]);

        // W2 fix: check Shell_NotifyIconW return value
        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            tracing::warn!("Shell_NotifyIconW(NIM_ADD) failed — tray icon may not be visible");
        }

        QUIT_TX.with(|cell| {
            *cell.borrow_mut() = Some(quit_tx);
        });

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }

        Shell_NotifyIconW(NIM_DELETE, &nid);

        Ok(())
    }

    thread_local! {
        static QUIT_TX: std::cell::RefCell<Option<SyncSender<()>>> = std::cell::RefCell::new(None);
    }

    unsafe extern "system" fn tray_wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_TRAYICON => {
                let event = (lparam.0 & 0xFFFF) as u32;
                // W3 fix: use named constant instead of magic number
                if event == WM_RBUTTONUP {
                    show_context_menu(hwnd);
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let id = (wparam.0 & 0xFFFF) as u16;
                if id == IDM_QUIT {
                    QUIT_TX.with(|cell| {
                        if let Some(tx) = cell.borrow().as_ref() {
                            let _ = tx.send(());
                        }
                    });
                    DestroyWindow(hwnd).ok();
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe fn show_context_menu(hwnd: HWND) {
        let menu = CreatePopupMenu().expect("Failed to create popup menu");
        AppendMenuW(menu, MF_STRING, IDM_QUIT as usize, w!("Quit")).ok();

        let mut pt = Default::default();
        GetCursorPos(&mut pt).ok();

        SetForegroundWindow(hwnd).ok();
        TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, None).ok();

        // C3 fix: destroy menu to prevent handle leak
        DestroyMenu(menu).ok();
    }

    unsafe fn load_embedded_icon() -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
        use windows::Win32::UI::WindowsAndMessaging::{CreateIconFromResourceEx, LR_DEFAULTCOLOR};

        let ico_data = super::TRAY_ICON_BYTES;

        // ICO header: 6 bytes, each directory entry: 16 bytes
        // The image data offset is stored as u32 LE at bytes [18..22] of the first entry
        if ico_data.len() < 22 {
            return None;
        }

        // C1 fix: read actual image offset from ICO directory entry
        let image_offset =
            u32::from_le_bytes([ico_data[18], ico_data[19], ico_data[20], ico_data[21]]) as usize;

        if image_offset >= ico_data.len() {
            return None;
        }

        let bmp_data = &ico_data[image_offset..];

        let icon = CreateIconFromResourceEx(
            bmp_data,
            true,       // fIcon
            0x00030000, // version
            16,
            16,
            LR_DEFAULTCOLOR,
        );
        icon.ok()
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::WindowsTray;
