use tracing::{info, warn};

#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_MEDIA_PLAY_PAUSE,
};

/// Sends global media transport commands to the operating system.
///
/// On Windows the preferred path is the System Media Transport Controls session
/// (`Windows.Media.Control`), which allows an explicit pause/resume of the
/// active session (e.g. a YouTube tab). When no session is available the code
/// falls back to synthesising the media play/pause key.
#[derive(Default)]
pub struct MediaController;

impl MediaController {
    pub fn new() -> Self {
        #[cfg(target_os = "windows")]
        init_com();
        Self
    }

    /// Pauses the current media session.
    pub fn pause_media(&self) {
        info!("Sending MediaPause to system");
        if !self.pause_via_session() {
            self.send_media_key();
        }
    }

    /// Resumes the current media session.
    pub fn play_media(&self) {
        info!("Sending MediaPlay to system");
        if !self.play_via_session() {
            self.send_media_key();
        }
    }

    /// Toggles play/pause — used when the target session state is unknown.
    pub fn toggle_play_pause(&self) {
        info!("Sending MediaPlayPause (toggle) to system");
        self.send_media_key();
    }

    #[cfg(target_os = "windows")]
    fn pause_via_session(&self) -> bool {
        match windows_media::pause() {
            Ok(()) => true,
            Err(err) if is_no_session(&err) => {
                tracing::debug!("No SMTC media session to pause");
                false
            }
            Err(err) => {
                warn!("Media session pause unavailable ({err}); falling back to media key");
                false
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn play_via_session(&self) -> bool {
        match windows_media::play() {
            Ok(()) => true,
            Err(err) if is_no_session(&err) => {
                tracing::debug!("No SMTC media session to resume");
                false
            }
            Err(err) => {
                warn!("Media session play unavailable ({err}); falling back to media key");
                false
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn pause_via_session(&self) -> bool {
        false
    }

    #[cfg(not(target_os = "windows"))]
    fn play_via_session(&self) -> bool {
        false
    }

    fn send_media_key(&self) {
        #[cfg(target_os = "windows")]
        unsafe {
            keybd_event(VK_MEDIA_PLAY_PAUSE.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
            keybd_event(VK_MEDIA_PLAY_PAUSE.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        }

        #[cfg(target_os = "linux")]
        run_command("playerctl", &["play-pause"]);

        #[cfg(target_os = "macos")]
        run_command(
            "osascript",
            &["-e", "tell application \"Music\" to playpause"],
        );

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        warn!("Media control is not implemented for this platform");
    }
}

#[cfg(target_os = "windows")]
fn is_no_session(err: &windows::core::Error) -> bool {
    // `GetCurrentSession` reports "no session" by returning S_OK with a null
    // interface, which windows-rs surfaces as an `Error` with HRESULT 0.
    err.code().0 == 0
}

#[cfg(target_os = "windows")]
fn init_com() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    // S_OK / S_FALSE on success, RPC_E_CHANGED_MODE when another apartment was
    // already set up on this thread — all of them are fine for WinRT
    // activation. Failures surface later, when the media session is queried.
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    tracing::debug!("CoInitializeEx returned 0x{:08X}", hr.0);
}

#[cfg(target_os = "windows")]
mod windows_media {
    use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

    fn current_session(
    ) -> windows::core::Result<windows::Media::Control::GlobalSystemMediaTransportControlsSession>
    {
        let manager =
            GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.get()?;
        manager.GetCurrentSession()
    }

    pub fn pause() -> windows::core::Result<()> {
        let session = current_session()?;
        let _was_playing = session.TryPauseAsync()?.get()?;
        Ok(())
    }

    pub fn play() -> windows::core::Result<()> {
        let session = current_session()?;
        let _was_paused = session.TryPlayAsync()?.get()?;
        Ok(())
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_command(program: &str, args: &[&str]) {
    match std::process::Command::new(program).args(args).status() {
        Ok(status) if status.success() => {}
        Ok(status) => warn!("{program} exited with {status}"),
        Err(err) => warn!("Failed to run {program}: {err}"),
    }
}
