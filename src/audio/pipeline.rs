use std::collections::VecDeque;

use crossbeam_channel::Sender;
use rustfft::num_complex::Complex;

use crate::audio::eq::Equalizer;
use crate::audio::volume::VolumeControl;
use crate::util::channels::AudioEvent;

const FFT_SIZE: usize = 2048;
pub const NUM_BARS: usize = 32;

// ── SpectrumAnalyser ─────────────────────────────────────────────────────────

/// Pre-allocates all FFT buffers and pre-computes per-bar bin ranges so the
/// hot decode loop performs zero heap allocation per spectrum frame.
pub struct SpectrumAnalyser {
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    /// Scratch complex buffer reused every call.
    input: Vec<Complex<f32>>,
    /// Magnitude scratch buffer (positive-frequency half).
    magnitudes: Vec<f32>,
    /// Output bars, overwritten in place.
    bars: [f32; NUM_BARS],
    /// Mono accumulator — samples are pushed here from the decode loop.
    fft_mono_buf: VecDeque<f32>,
    /// Pre-computed (bin_low, bin_high) inclusive ranges for each bar.
    bin_ranges: [(usize, usize); NUM_BARS],
    /// Hann window coefficients, pre-computed once.
    hann: Vec<f32>,
}

impl SpectrumAnalyser {
    pub fn new(sample_rate: u32) -> Self {
        let mut planner = rustfft::FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);

        let hann: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                0.5 * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32
                        / FFT_SIZE as f32)
                        .cos())
            })
            .collect();

        let half = FFT_SIZE / 2;
        let min_freq = 50.0_f32;
        let max_freq = (sample_rate as f32 / 2.0).min(16_000.0);
        let freq_per_bin = sample_rate as f32 / FFT_SIZE as f32;
        let log_ratio = (max_freq / min_freq).ln();

        let mut bin_ranges = [(0_usize, 0_usize); NUM_BARS];
        for (i, range) in bin_ranges.iter_mut().enumerate() {
            let f_low =
                min_freq * (log_ratio * i as f32 / NUM_BARS as f32).exp();
            let f_high = min_freq
                * (log_ratio * (i + 1) as f32 / NUM_BARS as f32).exp();
            let bin_low = (f_low / freq_per_bin) as usize;
            let bin_high = ((f_high / freq_per_bin) as usize).min(half - 1);
            *range = (bin_low, bin_high);
        }

        Self {
            fft,
            input: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            magnitudes: vec![0.0; FFT_SIZE / 2],
            bars: [0.0; NUM_BARS],
            fft_mono_buf: VecDeque::with_capacity(FFT_SIZE * 2),
            bin_ranges,
            hann,
        }
    }

    /// Clear the FFT accumulator (e.g. after a seek).
    pub fn reset(&mut self) {
        self.fft_mono_buf.clear();
    }

    /// Push interleaved samples into the mono accumulator and run **all**
    /// pending FFT hops so the deque stays bounded.  Returns `Some(&bars)`
    /// with the **most recent** spectrum frame, or `None` if not enough data.
    pub fn push_and_compute(
        &mut self,
        samples: &[f32],
        channels: usize,
    ) -> Option<&[f32; NUM_BARS]> {
        // Down-mix to mono and append.
        for chunk in samples.chunks(channels) {
            let mono: f32 = chunk.iter().sum::<f32>() / channels as f32;
            self.fft_mono_buf.push_back(mono);
        }

        // Cap backlog: if the deque grew beyond a sane budget, drop the
        // oldest samples so the next FFT reflects recent audio.
        const MAX_BACKLOG: usize = FFT_SIZE * 4;
        if self.fft_mono_buf.len() > MAX_BACKLOG {
            let excess = self.fft_mono_buf.len() - MAX_BACKLOG;
            self.fft_mono_buf.drain(..excess);
        }

        // Run every available FFT hop so we never accumulate lag.
        let hop = FFT_SIZE / 4;
        let half = FFT_SIZE / 2;
        let norm = FFT_SIZE as f32;
        let mut produced = false;

        while self.fft_mono_buf.len() >= FFT_SIZE {
            let contiguous = self.fft_mono_buf.make_contiguous();
            for (i, (dst, &src)) in self
                .input
                .iter_mut()
                .zip(contiguous[..FFT_SIZE].iter())
                .enumerate()
            {
                *dst = Complex::new(src * self.hann[i], 0.0);
            }

            self.fft_mono_buf.drain(..hop);
            self.fft.process(&mut self.input);

            for (mag, c) in
                self.magnitudes.iter_mut().zip(self.input[..half].iter())
            {
                *mag = c.norm();
            }

            // Map magnitudes to bars (dB scale, -60→0 dB → 0.0→1.0).
            for (bar, &(bin_low, bin_high)) in
                self.bars.iter_mut().zip(self.bin_ranges.iter())
            {
                let mut peak = 0.0_f32;
                for bin in bin_low..=bin_high {
                    if bin < half {
                        peak = peak.max(self.magnitudes[bin]);
                    }
                }
                let raw = peak / norm;
                let db = (20.0 * (raw + 1e-10_f32).log10()).max(-80.0);
                *bar = ((db + 80.0) / 80.0).clamp(0.0, 1.0);
            }
            produced = true;
        }

        if produced { Some(&self.bars) } else { None }
    }
}

// ── DspChain ─────────────────────────────────────────────────────────────────

/// Bundles all per-sample processing: volume ramping, EQ, and RMS metering.
/// Spectrum analysis is handled separately on the output side for accurate
/// synchronisation with what the listener actually hears.
pub struct DspChain {
    pub volume: VolumeControl,
    /// 10-band graphic equalizer — sits between volume and metering so the
    /// level reflects the post-EQ signal.
    pub eq: Equalizer,
    /// ReplayGain linear multiplier (1.0 = no change).
    replay_gain: f32,
}

impl DspChain {
    /// Create a new DSP chain for the given stream parameters.
    pub fn new(sample_rate: u32, channels: u16, initial_volume: f32) -> Self {
        Self {
            volume: VolumeControl::new(initial_volume, sample_rate),
            eq: Equalizer::new(sample_rate, channels),
            replay_gain: 1.0,
        }
    }

    /// Set the ReplayGain linear multiplier.
    pub fn set_replay_gain(&mut self, gain: f32) {
        self.replay_gain = gain;
    }

    /// Run the full DSP chain on a buffer of interleaved samples.
    /// Sends Level events to the UI as a side-effect.
    pub fn process(&mut self, samples: &mut Vec<f32>, event_tx: &Sender<AudioEvent>) {
        // 1. ReplayGain normalization (before volume so user control sits on top).
        if self.replay_gain != 1.0 {
            for s in samples.iter_mut() {
                *s *= self.replay_gain;
            }
        }

        // 2. Volume — applied in the cpal callback (post-ring) for instant response.

        // 3. Graphic equalizer (no-op when disabled).
        self.eq.process(samples);

        // 4. RMS level metering.
        if !samples.is_empty() {
            let sum: f32 = samples.iter().map(|s| s * s).sum();
            let rms = (sum / samples.len() as f32).sqrt();
            let _ = event_tx.send(AudioEvent::Level(rms));
        }

        // 5. Hard clamp — prevent DAC clipping when EQ/gain push past ±1.0.
        //    Explicit NaN/Inf guard: f32::clamp passes NaN through.
        for s in samples.iter_mut() {
            *s = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
        }
    }
}
