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
struct SpectrumAnalyser {
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    /// Scratch complex buffer reused every call.
    input: Vec<Complex<f32>>,
    /// Magnitude scratch buffer (positive-frequency half).
    magnitudes: Vec<f32>,
    /// Output bars, overwritten in place.
    bars: [f32; NUM_BARS],
    /// Mono accumulator — samples are pushed here from the decode loop.
    fft_mono_buf: Vec<f32>,
    /// Pre-computed (bin_low, bin_high) inclusive ranges for each bar.
    bin_ranges: [(usize, usize); NUM_BARS],
    /// Hann window coefficients, pre-computed once.
    hann: Vec<f32>,
}

impl SpectrumAnalyser {
    fn new(sample_rate: u32) -> Self {
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
            fft_mono_buf: Vec::with_capacity(FFT_SIZE * 2),
            bin_ranges,
            hann,
        }
    }

    /// Push interleaved samples into the mono accumulator.
    /// Returns `Some(&bars)` when a full FFT frame is ready and processed.
    fn push_and_compute(
        &mut self,
        samples: &[f32],
        channels: usize,
    ) -> Option<&[f32; NUM_BARS]> {
        for chunk in samples.chunks(channels) {
            let mono: f32 = chunk.iter().sum::<f32>() / channels as f32;
            self.fft_mono_buf.push(mono);
        }

        if self.fft_mono_buf.len() < FFT_SIZE {
            return None;
        }

        for (i, (dst, &src)) in self
            .input
            .iter_mut()
            .zip(self.fft_mono_buf[..FFT_SIZE].iter())
            .enumerate()
        {
            *dst = Complex::new(src * self.hann[i], 0.0);
        }

        self.fft_mono_buf.drain(..FFT_SIZE);
        self.fft.process(&mut self.input);

        let half = FFT_SIZE / 2;
        for (mag, c) in self.magnitudes.iter_mut().zip(self.input[..half].iter()) {
            *mag = c.norm();
        }

        let norm = FFT_SIZE as f32 * 0.5; // normalize by half FFT size
        for (bar, &(bin_low, bin_high)) in
            self.bars.iter_mut().zip(self.bin_ranges.iter())
        {
            let mut sum = 0.0_f32;
            let mut count = 0_usize;
            for bin in bin_low..=bin_high {
                if bin < half {
                    sum += self.magnitudes[bin];
                    count += 1;
                }
            }
            let raw = if count > 0 { sum / (count as f32 * norm) } else { 0.0 };
            let db = (20.0 * (raw + 1e-10_f32).log10()).max(-80.0);
            *bar = ((db + 80.0) / 70.0).clamp(0.0, 1.0);
        }

        Some(&self.bars)
    }
}

// ── DspChain ─────────────────────────────────────────────────────────────────

/// Bundles all per-sample processing: volume ramping, EQ, RMS metering, and
/// spectrum analysis. Owns the stages so the engine thread carries zero
/// DSP state itself.
pub struct DspChain {
    pub volume: VolumeControl,
    /// 10-band graphic equalizer — sits between volume and metering so the
    /// level and spectrum reflect the post-EQ signal.
    pub eq: Equalizer,
    analyser: SpectrumAnalyser,
    channels: usize,
}

impl DspChain {
    /// Create a new DSP chain for the given stream parameters.
    pub fn new(sample_rate: u32, channels: u16, initial_volume: f32) -> Self {
        Self {
            volume: VolumeControl::new(initial_volume, sample_rate),
            eq: Equalizer::new(sample_rate, channels),
            analyser: SpectrumAnalyser::new(sample_rate),
            channels: channels as usize,
        }
    }

    /// Run the full DSP chain on a buffer of interleaved samples.
    /// Sends Level and Spectrum events to the UI as side-effects.
    pub fn process(&mut self, samples: &mut Vec<f32>, event_tx: &Sender<AudioEvent>) {
        // 1. Volume ramping.
        self.volume.apply(samples);

        // 2. Graphic equalizer (no-op when disabled).
        self.eq.process(samples);

        // 3. RMS level metering.
        if !samples.is_empty() {
            let sum: f32 = samples.iter().map(|s| s * s).sum();
            let rms = (sum / samples.len() as f32).sqrt();
            let _ = event_tx.send(AudioEvent::Level(rms));
        }

        // 4. Spectrum analysis (produces bars when enough samples accumulate).
        if let Some(bars) = self.analyser.push_and_compute(samples, self.channels) {
            let _ = event_tx.send(AudioEvent::Spectrum(*bars));
        }
    }
}
