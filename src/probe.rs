//! `--probe` diagnostics: prints the loudest audio sessions and the Discord
//! peak level without pausing anything, so the audio path can be verified
//! while other players keep running normally.

use std::collections::HashSet;
use std::thread;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::BOOL;
use windows::Win32::Media::Audio::eCapture;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eConsole, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

use crate::audio::session_meter::SessionPeakMeter;
use crate::process::ProcessDetector;

const PROBE_DURATION: Duration = Duration::from_secs(30);
const SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

/// Restores the default microphone. Kept as a recovery switch in case a
/// previous mute experiment was interrupted before restoring the state.
pub fn unmute_microphone() {
    set_microphone_mute(false);
}

fn set_microphone_mute(mute: bool) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(enumerator) => enumerator,
                Err(err) => {
                    println!("cannot open the device enumerator: {err}");
                    return;
                }
            };
        let device = match enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) {
            Ok(device) => device,
            Err(err) => {
                println!("cannot open the default microphone: {err}");
                return;
            }
        };
        let volume: IAudioEndpointVolume = match device.Activate(CLSCTX_ALL, None) {
            Ok(volume) => volume,
            Err(err) => {
                println!("cannot read the microphone volume: {err}");
                return;
            }
        };

        let previous = volume.GetMute().map(|mute| mute.as_bool()).unwrap_or(false);
        if let Err(err) = volume.SetMute(BOOL(mute as i32), std::ptr::null()) {
            println!("cannot change the microphone mute state: {err}");
            return;
        }

        let now = volume.GetMute().map(|m| m.as_bool()).unwrap_or(!mute);
        println!("microphone muted: {now} (was: {previous})");
    }
}

pub fn run() {
    let meter = match SessionPeakMeter::new() {
        Ok(meter) => meter,
        Err(err) => {
            println!("cannot open the audio session meter: {err}");
            return;
        }
    };

    let mut detector = ProcessDetector::new();
    let start = Instant::now();

    println!("probing audio sessions for {} s (no media control)", PROBE_DURATION.as_secs());

    while start.elapsed() < PROBE_DURATION {
        detector.refresh();
        let discord_pids: HashSet<u32> = detector
            .pids_matching(&[
                "Discord.exe".to_string(),
                "discord".to_string(),
                "DiscordCanary.exe".to_string(),
                "DiscordPTB.exe".to_string(),
            ])
            .into_iter()
            .collect();

        let loudest = meter.loudest_sessions(4);
        let discord_peak = meter.peak_for_pids(&discord_pids);
        let capture_loudest = meter.loudest_sessions_for(eCapture, 3);

        let described: Vec<String> = loudest
            .iter()
            .map(|(pid, peak)| {
                let name = detector
                    .process_name(*pid)
                    .unwrap_or_else(|| "?".to_string());
                let tag = if discord_pids.contains(pid) { " DISCORD" } else { "" };
                format!("{name}({pid}) {peak:.4}{tag}")
            })
            .collect();

        let inputs: Vec<String> = capture_loudest
            .iter()
            .map(|(pid, peak)| {
                let name = detector
                    .process_name(*pid)
                    .unwrap_or_else(|| "?".to_string());
                format!("{name}({pid}) {peak:.4}")
            })
            .collect();

        println!(
            "{:>6.1}s  discord_out={:.4}  out: {}  ||  mic_in: {}",
            start.elapsed().as_secs_f32(),
            discord_peak,
            described.join(" | "),
            inputs.join(" | ")
        );

        thread::sleep(SAMPLE_INTERVAL);
    }
}
