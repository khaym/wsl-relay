use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use wsl_relay::tray::{StubTray, TRAY_ICON_BYTES, TrayBackend};

#[test]
fn stub_tray_does_not_send_quit_signal() {
    // C5 fix: use Barrier for proper synchronization instead of sleep
    let (quit_tx, quit_rx) = mpsc::sync_channel::<()>(1);
    let barrier = Arc::new(Barrier::new(2));
    let barrier_clone = barrier.clone();

    let _handle = std::thread::spawn(move || {
        // Signal that the thread has started before parking
        barrier_clone.wait();
        let stub = StubTray;
        stub.run(quit_tx).unwrap();
    });

    // Wait until the spawned thread has started
    barrier.wait();
    // Give it a moment to enter park()
    std::thread::sleep(std::time::Duration::from_millis(10));

    // StubTray should be parked — quit_rx should NOT have received anything
    assert!(
        quit_rx.try_recv().is_err(),
        "StubTray should not send quit signal"
    );

    // Unpark the thread; due to loop { park() }, we need to drop quit_rx
    // so that run() can never complete (it loops forever).
    // Instead, just leak the thread — it will be cleaned up on process exit.
    drop(quit_rx);
}

#[test]
fn stub_tray_implements_send_sync() {
    // Compile-time contract test: StubTray must be Send + Sync
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<StubTray>();
}

#[test]
fn tray_icon_bytes_is_valid_ico() {
    // ICO files start with: reserved (2 bytes, 0x0000), type (2 bytes, 0x0001), count (2 bytes)
    assert!(TRAY_ICON_BYTES.len() > 22, "ICO file too small");
    assert_eq!(
        TRAY_ICON_BYTES[0..4],
        [0x00, 0x00, 0x01, 0x00],
        "Invalid ICO magic bytes"
    );
}

#[test]
fn tray_icon_bytes_has_expected_images() {
    let count = u16::from_le_bytes([TRAY_ICON_BYTES[4], TRAY_ICON_BYTES[5]]);
    assert_eq!(count, 4, "ICO should contain 16, 32, 48, 256 images");
}
