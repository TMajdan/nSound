//! Background worker: watches communicator processes, reads the level of the
//! audio *they are playing* and pauses/resumes the user's media accordingly.

use std::collections::HashSet;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tracing::{debug, info, warn};

#[cfg(target_os = "windows")]
use windows::Win32::Media::Audio::eCapture;

use crate::audio::duplicate::DuplicateSuppressor;
use crate::audio::session_meter::SessionPeakMeter;
use crate::audio::vad::{display_db, VoiceActivityDetector};
use crate::config::{AppConfig, AppStatus};
use crate::media::MediaController;
use crate::process::ProcessDetector;

/// How often the audio peak is sampled.
const POLL_INTERVAL: Duration = Duration::from_millis(25);
/// How often the process list is refreshed.
const PROCESS_REFRESH_INTERVAL: Duration = Duration::from_millis(1000);

pub struct AudioMonitor;

impl AudioMonitor {
    pub fn start_background_loop(config: Arc<Mutex<AppConfig>>, status: Arc<Mutex<AppStatus>>) {
        thread::spawn(move || Self::run(config, status));
    }

    fn run(config: Arc<Mutex<AppConfig>>, status: Arc<Mutex<AppStatus>>) {
        let mut detector = ProcessDetector::new();
        let mut vad = VoiceActivityDetector::new();
        let mut duplicate_suppressor = DuplicateSuppressor::new();
        let media = MediaController::new();

        let mut was_speaking = false;
        let mut pids: HashSet<u32> = HashSet::new();
        let mut active_names: Vec<String> = Vec::new();
        let mut last_process_refresh: Option<Instant> = None;

        // Diagnostics: also react to every audio session, not only the
        // communicator's. Useful to verify the audio path works at all.
        let capture_all_audio = std::env::var_os("NSOUND_CAPTURE_ALL_AUDIO").is_some();

        let meter = match SessionPeakMeter::new() {
            Ok(meter) => Some(meter),
            Err(err) => {
                warn!("Cannot initialise the audio session meter: {err}");
                None
            }
        };

        if capture_all_audio {
            warn!("NSOUND_CAPTURE_ALL_AUDIO is set: reacting to audio from every application");
        }
        info!("Monitoring communicator playback audio (microphone is not used)");

        loop {
            let (master_enabled, communicators, threshold, attack_ms, hold_ms, auto_resume) = {
                let cfg = config.lock();
                (
                    cfg.master_enabled,
                    cfg.communicators.clone(),
                    cfg.rms_threshold,
                    cfg.attack_ms,
                    cfg.hold_time_ms,
                    cfg.auto_resume,
                )
            };

            // --- Process discovery (once per second) -------------------------
            let refresh_due = last_process_refresh
                .map(|last| last.elapsed() >= PROCESS_REFRESH_INTERVAL)
                .unwrap_or(true);

            if refresh_due && master_enabled {
                let enabled: Vec<_> = communicators
                    .iter()
                    .filter(|c| c.enabled)
                    .cloned()
                    .collect();
                let per_communicator = detector.communicator_pids(&enabled);

                pids.clear();
                active_names.clear();
                let mut running_map = std::collections::HashMap::new();

                for comm in &communicators {
                    let comm_pids = per_communicator
                        .get(&comm.id)
                        .cloned()
                        .unwrap_or_default();
                    let running = !comm_pids.is_empty();
                    running_map.insert(comm.id.clone(), running);

                    if comm.enabled && running {
                        active_names.push(comm.name.clone());
                        pids.extend(comm_pids);
                    }
                }

                status.lock().communicator_running = running_map;
                last_process_refresh = Some(Instant::now());
                debug!(
                    "params: threshold={threshold} attack={attack_ms}ms hold={hold_ms}ms, communicator pids={:?}",
                    pids
                );
                if let Some(meter) = &meter {
                    debug!(
                        "loudest sessions (pid, peak): {:?}",
                        meter.loudest_sessions(5)
                    );
                }
            }

            // --- Guard: disabled or idle -------------------------------------
            let should_monitor = master_enabled && !pids.is_empty();

            if !should_monitor {
                if was_speaking {
                    if auto_resume {
                        info!("Monitoring stopped while voice was active - resuming media");
                        media.play_media();
                    }
                    was_speaking = false;
                }

                {
                    let mut st = status.lock();
                    st.is_monitoring = false;
                    st.is_speaking = false;
                    st.current_rms = 0.0;
                    st.current_db = -100.0;
                    st.media_paused = false;
                    st.status_message = if master_enabled {
                        "Waiting for an enabled communicator (e.g. Discord)...".to_string()
                    } else {
                        "Monitoring disabled (master switch OFF)".to_string()
                    };
                }

                thread::sleep(POLL_INTERVAL);
                continue;
            }

            // --- Audio level -------------------------------------------------
            // `render` is what the communicator sends to your speakers. Voice
            // apps can also re-render audio that already comes from somewhere
            // else: the local microphone (mic test / preview) or the audio of
            // another application (screen share with sound, activities). Such
            // duplicates are ignored, so only audio that originates on the call
            // itself counts.
            let (render_peak, mic_peak, other_peak) = match &meter {
                Some(meter) if capture_all_audio => (meter.peak_over_all_sessions(), 0.0, 0.0),
                Some(meter) => (
                    meter.peak_for_pids(&pids),
                    meter.peak_for_pids_for(eCapture, &pids),
                    meter.peak_excluding(&pids),
                ),
                None => (0.0, 0.0, 0.0),
            };

            // Audio that merely duplicates another source (other apps' playback
            // or the local microphone) never counts as somebody talking.
            let reference = mic_peak.max(other_peak);
            let peak = duplicate_suppressor.filter(reference, render_peak);

            debug!(
                "levels: render={render_peak:.4} reference={reference:.4} effective={peak:.4}"
            );

            let res = vad.process_buffer(
                &[peak],
                threshold,
                Duration::from_millis(attack_ms),
                Duration::from_millis(hold_ms),
            );

            let who = if active_names.is_empty() {
                "communicator".to_string()
            } else {
                active_names.join(", ")
            };

            if res.is_speaking && !was_speaking {
                info!(
                    "Voice detected in {who} audio ({:.1} dB) - pausing media",
                    res.db
                );
                media.pause_media();
                was_speaking = true;
            } else if !res.is_speaking && was_speaking {
                info!("{who} audio is silent again - resuming media");
                if auto_resume {
                    media.play_media();
                }
                was_speaking = false;
            }

            {
                let mut st = status.lock();
                st.is_monitoring = true;
                st.is_speaking = res.is_speaking;
                st.current_rms = res.rms;
                st.current_db = res.db;
                st.media_paused = was_speaking;
                st.status_message = if res.is_speaking {
                    format!(
                        "Voice on {who} ({:.1} dB) - media paused",
                        display_db(res.db)
                    )
                } else {
                    format!(
                        "Monitoring {who} audio - silent ({:.1} dB)",
                        display_db(res.db)
                    )
                };
            }

            thread::sleep(POLL_INTERVAL);
        }
    }
}
