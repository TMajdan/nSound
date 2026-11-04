use std::time::{Duration, Instant};

/// Calculates Root Mean Square (RMS) volume level of audio PCM samples.
pub fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();
    (sum_squares / samples.len() as f32).sqrt()
}

/// Converts an RMS amplitude value (0.0 to 1.0) into decibels (dBFS).
pub fn rms_to_db(rms: f32) -> f32 {
    let safe_rms = rms.max(0.00001); // avoid log10(0)
    20.0 * safe_rms.log10()
}

/// Below this the level is treated as silence in the UI.
const SILENCE_FLOOR_DB: f32 = -60.0;

/// Level shown in the UI: silence reads `0.0 dB` instead of `-100 dB`, which
/// users read as an error rather than as "nothing is coming in".
pub fn display_db(db: f32) -> f32 {
    if db <= SILENCE_FLOOR_DB {
        0.0
    } else {
        db
    }
}

/// Voice Activity Detection result.
#[derive(Debug, Clone, Copy)]
pub struct VadResult {
    pub rms: f32,
    pub db: f32,
    pub is_speaking: bool,
}

/// Evaluates voice activity with an attack delay and a hold (release) window.
pub struct VoiceActivityDetector {
    /// When the level first rose above the threshold in the current burst.
    speech_start: Option<Instant>,
    /// Last moment the level was above the threshold.
    last_speech_time: Option<Instant>,
    /// Latched state: set after the attack delay, cleared after the hold window.
    speaking: bool,
}

impl Default for VoiceActivityDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceActivityDetector {
    pub fn new() -> Self {
        Self {
            speech_start: None,
            last_speech_time: None,
            speaking: false,
        }
    }

    /// Processes a buffer of f32 samples.
    ///
    /// * `attack` — how long the level must stay above the threshold before
    ///   speech is reported, which suppresses short noise spikes.
    /// * `hold` — how long speech keeps being reported after the level drops,
    ///   so a pause in the middle of a sentence does not resume playback.
    pub fn process_buffer(
        &mut self,
        samples: &[f32],
        rms_threshold: f32,
        attack: Duration,
        hold: Duration,
    ) -> VadResult {
        self.process_buffer_at(samples, rms_threshold, attack, hold, Instant::now())
    }

    /// Same as [`Self::process_buffer`], but with an explicit clock so the
    /// state machine can be tested deterministically.
    pub fn process_buffer_at(
        &mut self,
        samples: &[f32],
        rms_threshold: f32,
        attack: Duration,
        hold: Duration,
        now: Instant,
    ) -> VadResult {
        let rms = calculate_rms(samples);
        let db = rms_to_db(rms);
        let above_threshold = rms >= rms_threshold;

        if above_threshold {
            // Every above-threshold sample postpones the release, no matter how
            // short the burst is — otherwise the hold window would start
            // counting from the last *completed* attack instead of from the
            // last moment somebody was actually audible.
            self.last_speech_time = Some(now);

            let burst_start = *self.speech_start.get_or_insert(now);
            if now.duration_since(burst_start) >= attack {
                self.speaking = true;
            }
        } else {
            self.speech_start = None;
            if let Some(last) = self.last_speech_time {
                if now.duration_since(last) >= hold {
                    self.speaking = false;
                }
            }
        }

        VadResult {
            rms,
            db,
            is_speaking: self.speaking,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ATTACK: Duration = Duration::from_millis(0);
    const HOLD: Duration = Duration::from_millis(400);

    #[test]
    fn rms_of_silence_is_zero() {
        let samples = vec![0.0f32; 64];
        assert_eq!(calculate_rms(&samples), 0.0);
        assert_eq!(rms_to_db(0.0), -100.0);
    }

    #[test]
    fn rms_of_full_scale_square_is_one() {
        let samples: Vec<f32> = (0..64)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        assert!((calculate_rms(&samples) - 1.0).abs() < 1e-6);
        assert!((rms_to_db(1.0)).abs() < 1e-4); // 0 dBFS
    }

    #[test]
    fn detects_voice_above_threshold() {
        let mut vad = VoiceActivityDetector::new();
        let loud = vec![0.2f32; 128];
        let res = vad.process_buffer(&loud, 0.02, ATTACK, HOLD);
        assert!(res.is_speaking);
        assert!(res.rms > 0.02);
    }

    #[test]
    fn attack_delay_suppresses_short_spikes() {
        let mut vad = VoiceActivityDetector::new();
        let loud = vec![0.2f32; 128];
        let attack = Duration::from_millis(200);
        let start = Instant::now();

        // The burst is not reported until the attack delay has elapsed.
        assert!(!vad
            .process_buffer_at(&loud, 0.02, attack, HOLD, start)
            .is_speaking);
        assert!(!vad
            .process_buffer_at(&loud, 0.02, attack, HOLD, start + Duration::from_millis(100))
            .is_speaking);
        assert!(vad
            .process_buffer_at(&loud, 0.02, attack, HOLD, start + Duration::from_millis(200))
            .is_speaking);
    }

    #[test]
    fn holds_speaking_state_until_hold_time_elapses() {
        let mut vad = VoiceActivityDetector::new();
        let loud = vec![0.2f32; 128];
        let quiet = vec![0.0f32; 128];
        let start = Instant::now();

        assert!(vad
            .process_buffer_at(&loud, 0.02, ATTACK, HOLD, start)
            .is_speaking);

        // Inside the hold window the speaking state is kept.
        assert!(vad
            .process_buffer_at(&quiet, 0.02, ATTACK, HOLD, start + Duration::from_millis(100))
            .is_speaking);

        // Once the hold window expires the state is released.
        assert!(!vad
            .process_buffer_at(&quiet, 0.02, ATTACK, HOLD, start + HOLD)
            .is_speaking);
    }

    /// Runtime defaults: attack 250 ms, hold 1200 ms, 10 ms buffers at the
    /// audio callback rate.
    const RUNTIME_ATTACK: Duration = Duration::from_millis(250);
    const RUNTIME_HOLD: Duration = Duration::from_millis(1200);
    const BUFFER: Duration = Duration::from_millis(10);

    #[test]
    fn short_noise_bursts_never_trigger_speech() {
        let mut vad = VoiceActivityDetector::new();
        let loud = vec![0.2f32; 480];
        let quiet = vec![0.0f32; 480];
        let start = Instant::now();

        // 5 s of noise crossing the threshold in 100 ms bursts.
        for step in 0..500u32 {
            let above = (step / 10) % 2 == 0;
            let samples = if above { &loud } else { &quiet };
            let res = vad.process_buffer_at(
                samples,
                0.02,
                RUNTIME_ATTACK,
                RUNTIME_HOLD,
                start + BUFFER * step,
            );
            assert!(!res.is_speaking, "false positive at step {step}");
        }
    }

    #[test]
    fn brief_activity_extends_the_hold_window() {
        let mut vad = VoiceActivityDetector::new();
        let loud = vec![0.2f32; 480];
        let quiet = vec![0.0f32; 480];
        let mut t = Instant::now();

        // 300 ms of speech: after the attack the detector reports speech.
        for _ in 0..30 {
            vad.process_buffer_at(&loud, 0.02, RUNTIME_ATTACK, RUNTIME_HOLD, t);
            t += BUFFER;
        }
        assert!(vad
            .process_buffer_at(&loud, 0.02, RUNTIME_ATTACK, RUNTIME_HOLD, t)
            .is_speaking);

        // 1000 ms of silence: still inside the hold window.
        for _ in 0..100 {
            assert!(vad
                .process_buffer_at(&quiet, 0.02, RUNTIME_ATTACK, RUNTIME_HOLD, t)
                .is_speaking);
            t += BUFFER;
        }

        // A single 100 ms blip (shorter than the attack) must refresh the hold.
        for _ in 0..10 {
            vad.process_buffer_at(&loud, 0.02, RUNTIME_ATTACK, RUNTIME_HOLD, t);
            t += BUFFER;
        }

        // Without the refresh the old deadline would already have expired.
        for _ in 0..100 {
            assert!(
                vad.process_buffer_at(&quiet, 0.02, RUNTIME_ATTACK, RUNTIME_HOLD, t)
                    .is_speaking,
                "hold window was not refreshed by short activity"
            );
            t += BUFFER;
        }

        // 1.5 s of silence finally releases it.
        let mut released = false;
        for _ in 0..150 {
            t += BUFFER;
            if !vad
                .process_buffer_at(&quiet, 0.02, RUNTIME_ATTACK, RUNTIME_HOLD, t)
                .is_speaking
            {
                released = true;
                break;
            }
        }
        assert!(released, "VAD never released the speaking state");
    }

    #[test]
    fn speech_with_short_gap_does_not_resume_early() {
        let mut vad = VoiceActivityDetector::new();
        let loud = vec![0.2f32; 480];
        let quiet = vec![0.0f32; 480];
        let start = Instant::now();

        // 1 s of speech.
        let mut t = start;
        for _ in 0..100 {
            vad.process_buffer_at(&loud, 0.02, RUNTIME_ATTACK, RUNTIME_HOLD, t);
            t += BUFFER;
        }
        assert!(vad
            .process_buffer_at(&loud, 0.02, RUNTIME_ATTACK, RUNTIME_HOLD, t)
            .is_speaking);

        // 300 ms gap — inside the hold window, playback must stay paused.
        for _ in 0..30 {
            let res = vad.process_buffer_at(&quiet, 0.02, RUNTIME_ATTACK, RUNTIME_HOLD, t);
            assert!(res.is_speaking, "resumed too early");
            t += BUFFER;
        }

        // 1.5 s of silence — the hold window has expired, resume is allowed.
        for _ in 0..150 {
            t += BUFFER;
            let res = vad.process_buffer_at(&quiet, 0.02, RUNTIME_ATTACK, RUNTIME_HOLD, t);
            if !res.is_speaking {
                return;
            }
        }
        panic!("VAD never released the speaking state");
    }
}
