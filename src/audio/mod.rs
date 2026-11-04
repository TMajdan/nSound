pub mod duplicate;
pub mod vad;

// Windows only for now: the level of the audio a communicator is *playing* is
// read from its WASAPI audio sessions, so the microphone is never involved.
#[cfg(target_os = "windows")]
pub mod monitor;
#[cfg(target_os = "windows")]
pub mod session_meter;
#[cfg(target_os = "windows")]
pub use monitor::AudioMonitor;
