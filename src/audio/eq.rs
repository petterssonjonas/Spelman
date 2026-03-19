/// 10-band graphic equalizer implemented as a series of biquad peaking-EQ
/// (bell) filters, one per band.  All arithmetic follows the Audio EQ Cookbook
/// by Robert Bristow-Johnson.
///
/// # Signal path
///
/// Interleaved samples arrive in `process`.  Each band filter is applied in
/// series so the total response is the sum of all band gains.  Processing is
/// skipped entirely when the EQ is disabled, adding zero overhead to the hot
/// path in that state.
///
/// # Example
///
/// ```no_run
/// use spelman::audio::eq::Equalizer;
///
/// let mut eq = Equalizer::new(44_100, 2);
/// eq.set_band_gain(2, 6.0);   // +6 dB at 310 Hz
/// eq.set_band_gain(7, -3.0);  // -3 dB at 12 kHz
/// eq.set_enabled(true);
/// ```
use std::f32::consts::PI;

/// Number of EQ bands.
pub const NUM_EQ_BANDS: usize = 10;

/// Centre frequencies (Hz) for each of the ten bands.
pub const EQ_FREQUENCIES: [f32; NUM_EQ_BANDS] =
    [60.0, 170.0, 310.0, 600.0, 1_000.0, 3_000.0, 6_000.0, 12_000.0, 14_000.0, 16_000.0];

/// Minimum / maximum gain that can be applied to a single band (dB).
const GAIN_MIN_DB: f32 = -12.0;
const GAIN_MAX_DB: f32 = 12.0;

/// Fixed Q factor used for all bands.
const Q: f32 = 1.41;

/// Maximum number of channels supported by the per-sample state arrays.
/// Supports mono, stereo, and surround (up to 7.1).
const MAX_CHANNELS: usize = 8;

// ── BiquadFilter ──────────────────────────────────────────────────────────────

/// A single second-order IIR section implementing a peaking-EQ response.
///
/// Coefficients are stored in normalised form (divided by `a0`).  State is
/// kept per channel so a stereo stream can be processed correctly without
/// allocating separate filter instances.
pub struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    /// Input delay line, one slot per channel.
    x1: [f32; MAX_CHANNELS],
    x2: [f32; MAX_CHANNELS],
    /// Output delay line, one slot per channel.
    y1: [f32; MAX_CHANNELS],
    y2: [f32; MAX_CHANNELS],
}

impl BiquadFilter {
    /// Create a unity-gain (flat / bypass) filter.
    ///
    /// `b0 = 1`, all other coefficients zero — the filter passes every sample
    /// unchanged until `set_peaking_eq` is called.
    #[must_use]
    pub fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: [0.0; MAX_CHANNELS],
            x2: [0.0; MAX_CHANNELS],
            y1: [0.0; MAX_CHANNELS],
            y2: [0.0; MAX_CHANNELS],
        }
    }

    /// Compute and store peaking-EQ (bell) coefficients.
    ///
    /// Implements the peaking-EQ formulae from the Audio EQ Cookbook:
    ///
    /// ```text
    /// A      = 10^(gain_db / 40)
    /// w0     = 2·π·freq / sample_rate
    /// alpha  = sin(w0) / (2·Q)
    ///
    /// b0 =   1 + alpha·A
    /// b1 = –2·cos(w0)
    /// b2 =   1 – alpha·A
    /// a0 =   1 + alpha/A
    /// a1 = –2·cos(w0)
    /// a2 =   1 – alpha/A
    /// ```
    ///
    /// All stored coefficients are normalised by `a0`.
    ///
    /// When `gain_db` is 0 the filter degenerates to a flat (unity-gain)
    /// response; coefficients are set directly to avoid a divide-by-near-zero
    /// condition in the cookbook formula.
    pub fn set_peaking_eq(&mut self, sample_rate: f32, freq: f32, q: f32, gain_db: f32) {
        // Reset delay-line state whenever coefficients change so we do not
        // get a transient click from stale state that no longer matches the
        // new filter shape.
        self.x1 = [0.0; MAX_CHANNELS];
        self.x2 = [0.0; MAX_CHANNELS];
        self.y1 = [0.0; MAX_CHANNELS];
        self.y2 = [0.0; MAX_CHANNELS];

        if gain_db == 0.0 {
            // Flat — bypass coefficients.
            self.b0 = 1.0;
            self.b1 = 0.0;
            self.b2 = 0.0;
            self.a1 = 0.0;
            self.a2 = 0.0;
            return;
        }

        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        // Normalise by a0.
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    /// Process one sample for the given `channel` index using Direct Form I.
    ///
    /// `channel` must be less than [`MAX_CHANNELS`] (i.e. 0 or 1).
    #[inline]
    pub fn process_sample(&mut self, input: f32, channel: usize) -> f32 {
        debug_assert!(channel < MAX_CHANNELS, "channel index out of range");

        let output = self.b0 * input
            + self.b1 * self.x1[channel]
            + self.b2 * self.x2[channel]
            - self.a1 * self.y1[channel]
            - self.a2 * self.y2[channel];

        // Shift delay lines.
        self.x2[channel] = self.x1[channel];
        self.x1[channel] = input;
        self.y2[channel] = self.y1[channel];
        self.y1[channel] = output;

        output
    }
}

impl Default for BiquadFilter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Equalizer ─────────────────────────────────────────────────────────────────

/// 10-band graphic equalizer.
///
/// Bands are applied in series (cascaded biquads); each band is an independent
/// peaking-EQ centred at one of the ten [`EQ_FREQUENCIES`].  Gain range per
/// band is `–12 dB` to `+12 dB`.
///
/// Call [`Equalizer::process`] on each decoded buffer of interleaved samples.
/// When disabled (`enabled == false`) the method returns immediately without
/// modifying the buffer.
pub struct Equalizer {
    /// One biquad section per frequency band.
    pub bands: [BiquadFilter; NUM_EQ_BANDS],
    /// Current gain setting (dB) for each band, mirrored here so the UI can
    /// read them back without touching the filter coefficients.
    gains_db: [f32; NUM_EQ_BANDS],
    /// Whether the EQ is active.  When `false`, `process` is a no-op.
    enabled: bool,
    sample_rate: u32,
    channels: u16,
}

impl Equalizer {
    /// Create a new equalizer with all bands at 0 dB (flat response).
    ///
    /// The EQ starts **disabled**; call [`Equalizer::set_enabled`] to activate
    /// it.
    #[must_use]
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        // `BiquadFilter::new()` returns unity-gain coefficients, so no
        // `set_peaking_eq` call is needed until a band gain is changed.
        Self {
            bands: std::array::from_fn(|_| BiquadFilter::new()),
            gains_db: [0.0; NUM_EQ_BANDS],
            enabled: false,
            sample_rate,
            channels,
        }
    }

    /// Set the gain for a single `band` (0-indexed).
    ///
    /// `gain_db` is clamped to `[–12, +12]` before being applied.
    /// Recomputes the biquad coefficients for that band immediately.
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `band >= NUM_EQ_BANDS`.
    pub fn set_band_gain(&mut self, band: usize, gain_db: f32) {
        assert!(band < NUM_EQ_BANDS, "band index out of range");
        let clamped = gain_db.clamp(GAIN_MIN_DB, GAIN_MAX_DB);
        self.gains_db[band] = clamped;
        self.bands[band].set_peaking_eq(
            self.sample_rate as f32,
            EQ_FREQUENCIES[band],
            Q,
            clamped,
        );
    }

    /// Set all ten band gains at once.
    ///
    /// Each value is clamped to `[–12, +12]`.  All ten biquad sections are
    /// recomputed.
    pub fn set_all_gains(&mut self, gains: [f32; NUM_EQ_BANDS]) {
        for (band, &gain_db) in gains.iter().enumerate() {
            self.set_band_gain(band, gain_db);
        }
    }

    /// Enable or disable the equalizer.
    ///
    /// When disabled, [`process`](Equalizer::process) returns immediately
    /// without touching the sample buffer.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Return the current gain values (dB) for all bands.
    #[must_use]
    pub fn gains(&self) -> &[f32; NUM_EQ_BANDS] {
        &self.gains_db
    }

    /// Return whether the equalizer is currently enabled.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Apply the equalizer to a buffer of interleaved samples **in place**.
    ///
    /// Each sample is run through all ten biquad sections in series.  The
    /// channel index is derived from the sample position modulo `channels`.
    ///
    /// If the EQ is disabled this method returns immediately without reading
    /// or writing any sample data.
    pub fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }

        let channels = self.channels as usize;

        for (i, sample) in samples.iter_mut().enumerate() {
            let ch = i % channels;
            // Guard against mono/unexpected channel counts exceeding MAX_CHANNELS.
            let ch = ch.min(MAX_CHANNELS - 1);
            let mut s = *sample;
            for band in &mut self.bands {
                s = band.process_sample(s, ch);
            }
            *sample = s;
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat EQ (all gains 0 dB) must not alter any sample value.
    #[test]
    fn flat_eq_is_transparent() {
        let mut eq = Equalizer::new(44_100, 2);
        eq.set_enabled(true);

        let original: Vec<f32> = (0..256)
            .map(|i| (i as f32 / 256.0 * 2.0 - 1.0) * 0.5)
            .collect();
        let mut buf = original.clone();
        eq.process(&mut buf);

        for (expected, actual) in original.iter().zip(buf.iter()) {
            assert!(
                (expected - actual).abs() < 1e-6,
                "flat EQ changed sample: expected {expected}, got {actual}"
            );
        }
    }

    /// A disabled EQ must be a strict no-op (zero work done).
    #[test]
    fn disabled_eq_is_noop() {
        let mut eq = Equalizer::new(44_100, 2);
        // EQ is disabled by default; set a non-zero gain so it *would* change
        // the signal if it were active.
        eq.set_band_gain(0, 12.0);

        let original = vec![0.5_f32; 64];
        let mut buf = original.clone();
        eq.process(&mut buf); // must not modify buf

        assert_eq!(buf, original);
    }

    /// Positive gain on a single band should increase energy (not decrease it).
    #[test]
    fn positive_gain_increases_energy() {
        // Feed a sine wave at 60 Hz (band 0) and verify total power goes up.
        let sample_rate = 44_100_u32;
        let mut eq = Equalizer::new(sample_rate, 1);
        eq.set_band_gain(0, 12.0);
        eq.set_enabled(true);

        let freq = 60.0_f32;
        let samples: Vec<f32> = (0..4096)
            .map(|i| (2.0 * PI * freq * i as f32 / sample_rate as f32).sin() * 0.1)
            .collect();

        let rms_before: f32 = {
            let sum: f32 = samples.iter().map(|s| s * s).sum();
            (sum / samples.len() as f32).sqrt()
        };

        let mut buf = samples;
        // Discard transient (filter settling) — process twice.
        eq.process(&mut buf);
        eq.process(&mut buf);

        let rms_after: f32 = {
            let sum: f32 = buf.iter().map(|s| s * s).sum();
            (sum / buf.len() as f32).sqrt()
        };

        assert!(
            rms_after > rms_before,
            "positive gain did not increase energy: before={rms_before:.4}, after={rms_after:.4}"
        );
    }

    /// `set_all_gains` must clamp values outside [–12, +12].
    #[test]
    fn gains_are_clamped() {
        let mut eq = Equalizer::new(44_100, 2);
        let extreme = [100.0_f32; NUM_EQ_BANDS];
        eq.set_all_gains(extreme);
        for &g in eq.gains() {
            assert!(
                g <= GAIN_MAX_DB,
                "gain {g} exceeds GAIN_MAX_DB ({GAIN_MAX_DB})"
            );
        }

        let negative_extreme = [-100.0_f32; NUM_EQ_BANDS];
        eq.set_all_gains(negative_extreme);
        for &g in eq.gains() {
            assert!(
                g >= GAIN_MIN_DB,
                "gain {g} is below GAIN_MIN_DB ({GAIN_MIN_DB})"
            );
        }
    }

    /// `BiquadFilter::new()` with identity coefficients must be transparent.
    #[test]
    fn biquad_identity_passthrough() {
        let mut f = BiquadFilter::new();
        let samples = [0.1_f32, -0.2, 0.3, 0.0, 1.0];
        for &s in &samples {
            let out = f.process_sample(s, 0);
            assert!(
                (out - s).abs() < 1e-6,
                "identity filter changed sample: in={s}, out={out}"
            );
        }
    }
}
