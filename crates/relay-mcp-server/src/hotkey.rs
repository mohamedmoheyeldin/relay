//! The emergency stop hotkey (Ctrl+Alt+Shift+Esc) for the stand-alone server.

use relay_mcp::KillSwitch;

/// Register the hotkey on a background thread. Failure (another program, or Relay's own window,
/// already holds it) is logged and the server keeps working; the client's own stop still applies.
#[cfg(windows)]
pub fn spawn(kill: KillSwitch) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, VK_ESCAPE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    std::thread::spawn(move || {
        // SAFETY: plain Win32 calls on this thread; the message loop below owns the registration.
        unsafe {
            if RegisterHotKey(
                None,
                1,
                MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                VK_ESCAPE.0 as u32,
            )
            .is_err()
            {
                tracing::warn!("the stop hotkey is taken by another program");
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_HOTKEY {
                    tracing::warn!("emergency stop pressed");
                    kill.trip();
                }
            }
        }
    });
}

#[cfg(not(windows))]
pub fn spawn(_kill: KillSwitch) {
    tracing::info!("the global stop hotkey is not available in the stand-alone server on this OS");
}
