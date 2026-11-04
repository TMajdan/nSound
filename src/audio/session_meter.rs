//! Reads the playback level of audio rendered *by specific processes*.
//!
//! Windows exposes one audio session per process on every render endpoint, and
//! each session supports `IAudioMeterInformation`. That gives us the level of
//! what Discord is playing (other people talking) without touching the
//! microphone and without capturing the audio stream itself.

use std::collections::HashSet;

use tracing::debug;
use windows::core::Interface;
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{
    eRender, EDataFlow, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
};

pub struct SessionPeakMeter {
    enumerator: IMMDeviceEnumerator,
}

impl SessionPeakMeter {
    /// Creates the meter. Must be called from the thread that will poll it;
    /// COM is initialised as multi-threaded on that thread.
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            // S_FALSE (already initialised) is fine, RPC_E_CHANGED_MODE is not
            // fatal either: the calls below will report their own errors.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            Ok(Self { enumerator })
        }
    }

    /// Highest peak (0.0 - 1.0) among sessions whose process id is in `pids`.
    pub fn peak_for_pids(&self, pids: &HashSet<u32>) -> f32 {
        self.peak_for_pids_for(eRender, pids)
    }

    /// Highest peak (0.0 - 1.0) among sessions on `flow` whose process id is in
    /// `pids`. Used with `eCapture` to read the communicator's microphone.
    pub fn peak_for_pids_for(&self, flow: EDataFlow, pids: &HashSet<u32>) -> f32 {
        if pids.is_empty() {
            return 0.0;
        }
        let mut peak = 0.0f32;
        self.visit_sessions_for(flow, |pid, session_peak| {
            if pids.contains(&pid) && session_peak > peak {
                peak = session_peak;
            }
        });
        peak
    }

    /// Highest peak (0.0 - 1.0) among all sessions — diagnostics only, used by
    /// the `NSOUND_CAPTURE_ALL_AUDIO` troubleshooting switch.
    pub fn peak_over_all_sessions(&self) -> f32 {
        let mut peak = 0.0f32;
        self.visit_sessions(|_, session_peak| {
            if session_peak > peak {
                peak = session_peak;
            }
        });
        peak
    }

    /// Debug helper: the loudest sessions as `(process id, peak)` pairs.
    pub fn loudest_sessions(&self, limit: usize) -> Vec<(u32, f32)> {
        self.loudest_sessions_for(eRender, limit)
    }

    /// Debug helper for an explicit data flow (`eRender` playback, `eCapture`
    /// recording, i.e. microphones).
    pub fn loudest_sessions_for(&self, flow: EDataFlow, limit: usize) -> Vec<(u32, f32)> {
        let mut sessions: Vec<(u32, f32)> = Vec::new();
        self.visit_sessions_for(flow, |pid, peak| sessions.push((pid, peak)));
        sessions.sort_by(|a, b| b.1.total_cmp(&a.1));
        sessions.truncate(limit);
        sessions
    }

    /// Highest peak (0.0 - 1.0) among playback sessions whose process id is
    /// *not* in `pids` — i.e. everything else the user is listening to.
    pub fn peak_excluding(&self, pids: &HashSet<u32>) -> f32 {
        let mut peak = 0.0f32;
        self.visit_sessions_for(eRender, |pid, session_peak| {
            if !pids.contains(&pid) && session_peak > peak {
                peak = session_peak;
            }
        });
        peak
    }

    fn visit_sessions(&self, visit: impl FnMut(u32, f32)) {
        self.visit_sessions_for(eRender, visit)
    }

    fn visit_sessions_for(&self, flow: EDataFlow, mut visit: impl FnMut(u32, f32)) {
        unsafe {
            let devices = match self
                .enumerator
                .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
            {
                Ok(devices) => devices,
                Err(err) => {
                    debug!("Cannot enumerate render endpoints: {err}");
                    return;
                }
            };

            let device_count = devices.GetCount().unwrap_or(0);
            for device_index in 0..device_count {
                let Ok(device) = devices.Item(device_index) else {
                    continue;
                };
                let manager: IAudioSessionManager2 =
                    match device.Activate(CLSCTX_ALL, None) {
                        Ok(manager) => manager,
                        Err(err) => {
                            debug!("Cannot activate session manager: {err}");
                            continue;
                        }
                    };
                let Ok(sessions) = manager.GetSessionEnumerator() else {
                    continue;
                };
                let session_count = sessions.GetCount().unwrap_or(0);

                for session_index in 0..session_count {
                    let Ok(control) = sessions.GetSession(session_index) else {
                        continue;
                    };
                    let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
                        continue;
                    };
                    let Ok(pid) = control2.GetProcessId() else {
                        continue;
                    };
                    let Ok(meter) = control.cast::<IAudioMeterInformation>() else {
                        continue;
                    };
                    if let Ok(peak) = meter.GetPeakValue() {
                        visit(pid, peak);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_peak_value_from_the_mixer() {
        // Validates the whole WASAPI plumbing: device enumeration, session
        // enumeration and the per-session meter query. The value itself
        // depends on what is playing, so only its range is checked.
        let meter = match SessionPeakMeter::new() {
            Ok(meter) => meter,
            Err(err) => {
                eprintln!("skipping: no audio endpoint available ({err})");
                return;
            }
        };

        let peak = meter.peak_over_all_sessions();
        assert!(
            (0.0..=1.0).contains(&peak),
            "peak out of range: {peak}"
        );
    }
}
