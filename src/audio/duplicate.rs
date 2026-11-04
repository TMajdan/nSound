//! Ignores communicator output that is only a duplicate of what something else
//! is already playing.
//!
//! Voice apps can re-render audio that already has another source: the local
//! microphone (mic test / noise-suppression preview) or another application's
//! audio (screen share with sound, "watch together" activities). Without this
//! filter the user's own music — mirrored into the communicator's session —
//! looks exactly like somebody talking.
//!
//! The rule is a dominance test on smoothed levels: if the communicator is not
//! clearly louder than the loudest other source, its output is treated as a
//! copy and ignored. When somebody actually speaks on the call, the
//! communicator becomes much louder than the background media, and the signal
//! passes through.

use std::collections::VecDeque;

/// Smoothing factor for the exponential moving average (25 ms sampling → ~300 ms).
const SMOOTHING: f32 = 0.08;
/// Below this the "other source" is considered silent (just noise).
const MIN_REFERENCE: f32 = 0.02;
/// How much louder than the other source the communicator must be to count as
/// genuinely incoming audio.
const DOMINANCE_RATIO: f32 = 2.5;
/// Samples to ignore after start-up, until the averages have settled (~1 s).
const WARMUP_SAMPLES: usize = 40;
/// How long the reference level is held after it drops (~1 s). A communicator
/// mirroring another app keeps playing through short gaps in that app's level.
const REFERENCE_HOLD_SAMPLES: usize = 40;

pub struct DuplicateSuppressor {
    render_average: f32,
    reference_average: f32,
    reference_hold: VecDeque<f32>,
    samples_seen: usize,
}

impl Default for DuplicateSuppressor {
    fn default() -> Self {
        Self::new()
    }
}

impl DuplicateSuppressor {
    pub fn new() -> Self {
        Self {
            render_average: 0.0,
            reference_average: 0.0,
            reference_hold: VecDeque::with_capacity(REFERENCE_HOLD_SAMPLES),
            samples_seen: 0,
        }
    }

    /// `reference` is the loudest level produced by any other source (other
    /// applications' playback, the local microphone). Returns the level that
    /// should count as incoming communicator audio.
    pub fn filter(&mut self, reference: f32, render: f32) -> f32 {
        self.reference_hold.push_back(reference);
        if self.reference_hold.len() > REFERENCE_HOLD_SAMPLES {
            self.reference_hold.pop_front();
        }
        let held_reference = self
            .reference_hold
            .iter()
            .copied()
            .fold(0.0f32, f32::max);

        self.render_average += SMOOTHING * (render - self.render_average);
        self.reference_average += SMOOTHING * (held_reference - self.reference_average);

        self.samples_seen += 1;
        if self.samples_seen <= WARMUP_SAMPLES {
            // The averages are not meaningful yet — never react on start-up.
            return 0.0;
        }

        if self.reference_average < MIN_REFERENCE {
            return render;
        }

        let ratio = self.render_average / self.reference_average.max(1e-6);
        if ratio < DOMINANCE_RATIO {
            // The communicator is playing the same content as another source.
            0.0
        } else {
            render
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 25 ms sample of a music envelope, repeated for `steps` iterations.
    fn music(step: usize) -> f32 {
        if (step / 8) % 2 == 0 {
            0.18
        } else {
            0.10
        }
    }

    #[test]
    fn passes_audio_through_when_nothing_else_is_playing() {
        let mut suppressor = DuplicateSuppressor::new();
        let mut last = 0.0;
        for _ in 0..100 {
            last = suppressor.filter(0.0, 0.3);
        }
        assert!((last - 0.3).abs() < 1e-3, "expected passthrough, got {last}");
    }

    #[test]
    fn ignores_output_that_mirrors_another_application() {
        let mut suppressor = DuplicateSuppressor::new();
        let mut last = 1.0;
        for step in 0..200 {
            let reference = music(step);
            // Discord renders the very same music, only slightly louder.
            last = suppressor.filter(reference, reference * 1.2);
        }
        assert_eq!(last, 0.0, "duplicated media must be ignored");
    }

    #[test]
    fn keeps_voice_that_is_louder_than_the_background_media() {
        let mut suppressor = DuplicateSuppressor::new();
        // Music keeps playing for a while so the averages settle.
        for step in 0..200 {
            let reference = music(step);
            suppressor.filter(reference, reference * 1.2);
        }

        // Somebody talks on the call: much louder than the music.
        let mut last = 0.0;
        for step in 200..260 {
            let reference = music(step);
            last = suppressor.filter(reference, reference * 1.2 + 0.4);
        }
        assert!(last > 0.3, "voice must pass, got {last}");
    }
}
