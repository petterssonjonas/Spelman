/// Volume control with smooth ramping to avoid clicks/pops.
pub struct VolumeControl {
    /// Target volume (0.0 to 1.0).
    target: f32,
    /// Current volume (smoothly approaches target).
    current: f32,
    /// Ramp speed per sample (depends on sample rate).
    ramp_rate: f32,
}

impl VolumeControl {
    pub fn new(initial: f32, sample_rate: u32) -> Self {
        // Ramp over ~5ms to avoid clicks.
        let ramp_rate = 1.0 / (sample_rate as f32 * 0.005);
        Self {
            target: initial,
            current: initial,
            ramp_rate,
        }
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.target = vol.clamp(0.0, 1.0);
    }

    pub fn volume(&self) -> f32 {
        self.target
    }

    /// Apply volume to a buffer of interleaved samples, with smooth ramping.
    pub fn apply(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            // Ramp current toward target.
            if (self.current - self.target).abs() > self.ramp_rate {
                if self.current < self.target {
                    self.current += self.ramp_rate;
                } else {
                    self.current -= self.ramp_rate;
                }
            } else {
                self.current = self.target;
            }
            *sample *= self.current;
        }
    }
}
