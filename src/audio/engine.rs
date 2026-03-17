use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamConfig;
use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::audio::decoder::AudioDecoder;
use crate::audio::volume::VolumeControl;
use crate::util::channels::{AudioCommand, AudioEvent};

/// Shared state between decode thread and cpal callback via a lock-free ring buffer.
struct RingBuffer {
    buf: Box<[f32]>,
    /// Write position (decode thread).
    write_pos: AtomicU64,
    /// Read position (cpal callback).
    read_pos: AtomicU64,
}

impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0; capacity].into_boxed_slice(),
            write_pos: AtomicU64::new(0),
            read_pos: AtomicU64::new(0),
        }
    }

    fn capacity(&self) -> usize {
        self.buf.len()
    }

    fn available_read(&self) -> usize {
        let w = self.write_pos.load(Ordering::Acquire);
        let r = self.read_pos.load(Ordering::Acquire);
        (w - r) as usize
    }

    fn available_write(&self) -> usize {
        self.capacity() - self.available_read()
    }

    fn write(&self, data: &[f32]) -> usize {
        let available = self.available_write();
        let to_write = data.len().min(available);
        let cap = self.capacity();
        let w = self.write_pos.load(Ordering::Relaxed) as usize;

        for i in 0..to_write {
            let idx = (w + i) % cap;
            // SAFETY: single writer (decode thread), index always in bounds.
            unsafe {
                let ptr = self.buf.as_ptr() as *mut f32;
                ptr.add(idx).write(data[i]);
            }
        }

        self.write_pos
            .fetch_add(to_write as u64, Ordering::Release);
        to_write
    }

    fn read(&self, out: &mut [f32]) -> usize {
        let available = self.available_read();
        let to_read = out.len().min(available);
        let cap = self.capacity();
        let r = self.read_pos.load(Ordering::Relaxed) as usize;

        for i in 0..to_read {
            let idx = (r + i) % cap;
            // SAFETY: single reader (cpal callback), index always in bounds.
            unsafe {
                out[i] = *self.buf.as_ptr().add(idx);
            }
        }

        self.read_pos.fetch_add(to_read as u64, Ordering::Release);
        to_read
    }
}

// SAFETY: The ring buffer is designed for single-producer single-consumer use.
// The decode thread is the only writer and the cpal callback is the only reader.
// AtomicU64 operations provide the necessary synchronization.
unsafe impl Send for RingBuffer {}
unsafe impl Sync for RingBuffer {}

/// The audio engine manages decoding and playback.
pub struct AudioEngine {
    cmd_tx: Sender<AudioCommand>,
    event_rx: Receiver<AudioEvent>,
    _handle: thread::JoinHandle<()>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = unbounded::<AudioCommand>();
        let (event_tx, event_rx) = unbounded::<AudioEvent>();

        let handle = thread::Builder::new()
            .name("audio-engine".into())
            .spawn(move || {
                engine_thread(cmd_rx, event_tx);
            })
            .expect("Failed to spawn audio engine thread");

        Self {
            cmd_tx,
            event_rx,
            _handle: handle,
        }
    }

    pub fn send(&self, cmd: AudioCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn event_rx(&self) -> &Receiver<AudioEvent> {
        &self.event_rx
    }
}

fn engine_thread(cmd_rx: Receiver<AudioCommand>, event_tx: Sender<AudioEvent>) {
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            let _ = event_tx.send(AudioEvent::Error("No audio output device found".into()));
            return;
        }
    };

    let mut state = EngineState::Idle;
    let is_paused = Arc::new(AtomicBool::new(false));

    // FFT state for spectrum analysis.
    const FFT_SIZE: usize = 2048;
    const NUM_BARS: usize = 32;
    let mut fft_mono_buf: Vec<f32> = Vec::with_capacity(FFT_SIZE);
    let mut planner = rustfft::FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);

    loop {
        let is_idle = matches!(state, EngineState::Idle);

        if is_idle {
            // Block waiting for a command.
            match cmd_rx.recv() {
                Ok(AudioCommand::Play(path)) => {
                    state = start_playback(&path, &device, &event_tx, &is_paused);
                }
                Ok(_) => {}
                Err(_) => return,
            }
            continue;
        }

        // We're playing — process commands non-blocking.
        let mut new_state = None;
        if let EngineState::Playing {
            decoder,
            volume,
            stop_flag,
            ..
        } = &mut state
        {
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    AudioCommand::Play(path) => {
                        stop_flag.store(true, Ordering::Release);
                        new_state = Some(start_playback(
                            &path,
                            &device,
                            &event_tx,
                            &is_paused,
                        ));
                        break;
                    }
                    AudioCommand::Pause => {
                        is_paused.store(true, Ordering::Release);
                        let _ = event_tx.send(AudioEvent::Paused);
                    }
                    AudioCommand::Resume => {
                        is_paused.store(false, Ordering::Release);
                        let _ = event_tx.send(AudioEvent::Resumed);
                    }
                    AudioCommand::TogglePlayPause => {
                        let was_paused =
                            is_paused.fetch_xor(true, Ordering::AcqRel);
                        if was_paused {
                            let _ = event_tx.send(AudioEvent::Resumed);
                        } else {
                            let _ = event_tx.send(AudioEvent::Paused);
                        }
                    }
                    AudioCommand::Stop => {
                        stop_flag.store(true, Ordering::Release);
                        let _ = event_tx.send(AudioEvent::Stopped);
                        new_state = Some(EngineState::Idle);
                        break;
                    }
                    AudioCommand::Seek(pos) => {
                        if let Err(e) = decoder.seek(pos) {
                            let _ = event_tx
                                .send(AudioEvent::Error(e.to_string()));
                        }
                    }
                    AudioCommand::SetVolume(v) => {
                        volume.set_volume(v);
                    }
                }
            }
        }

        if let Some(ns) = new_state {
            state = ns;
            continue;
        }

        // Decode and fill ring buffer.
        if let EngineState::Playing {
            ring_buf,
            decoder,
            volume,
            stop_flag,
            samples_written,
            channels,
            sample_rate,
            ..
        } = &mut state
        {
            if ring_buf.available_write() > 4096 {
                match decoder.next_samples() {
                    Ok(Some(mut samples)) => {
                        volume.apply(&mut samples);

                        // Compute RMS for level meter.
                        let rms = if samples.is_empty() {
                            0.0
                        } else {
                            let sum: f32 =
                                samples.iter().map(|s| s * s).sum();
                            (sum / samples.len() as f32).sqrt()
                        };
                        let _ = event_tx.send(AudioEvent::Level(rms));

                        // Accumulate mono samples for FFT spectrum.
                        let ch = *channels as usize;
                        for chunk in samples.chunks(ch) {
                            let mono: f32 = chunk.iter().sum::<f32>() / ch as f32;
                            fft_mono_buf.push(mono);
                        }
                        if fft_mono_buf.len() >= FFT_SIZE {
                            let spectrum = compute_spectrum(
                                &fft_mono_buf[..FFT_SIZE],
                                &fft,
                                *sample_rate,
                                NUM_BARS,
                            );
                            let _ = event_tx.send(AudioEvent::Spectrum(spectrum));
                            fft_mono_buf.clear();
                        }

                        ring_buf.write(&samples);
                        samples_written.fetch_add(
                            samples.len() as u64,
                            Ordering::Relaxed,
                        );

                        let total_frames =
                            samples_written.load(Ordering::Relaxed)
                                / *channels as u64;
                        let pos_secs =
                            total_frames as f64 / *sample_rate as f64;
                        let _ = event_tx.send(AudioEvent::Position(
                            Duration::from_secs_f64(pos_secs),
                        ));
                    }
                    Ok(None) => {
                        // Track finished — wait for ring buffer to drain.
                        while ring_buf.available_read() > 0 {
                            thread::sleep(Duration::from_millis(10));
                        }
                        stop_flag.store(true, Ordering::Release);
                        let _ = event_tx.send(AudioEvent::Finished);
                        state = EngineState::Idle;
                    }
                    Err(e) => {
                        let _ =
                            event_tx.send(AudioEvent::Error(e.to_string()));
                        stop_flag.store(true, Ordering::Release);
                        state = EngineState::Idle;
                    }
                }
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

enum EngineState {
    Idle,
    Playing {
        ring_buf: Arc<RingBuffer>,
        decoder: AudioDecoder,
        volume: VolumeControl,
        stop_flag: Arc<AtomicBool>,
        samples_written: Arc<AtomicU64>,
        channels: u16,
        sample_rate: u32,
        _stream: cpal::Stream,
    },
}

fn start_playback(
    path: &Path,
    device: &cpal::Device,
    event_tx: &Sender<AudioEvent>,
    is_paused: &Arc<AtomicBool>,
) -> EngineState {
    let decoder = match AudioDecoder::open(path) {
        Ok(d) => d,
        Err(e) => {
            let _ = event_tx.send(AudioEvent::Error(e.to_string()));
            return EngineState::Idle;
        }
    };

    let info = decoder.info.clone();
    is_paused.store(false, Ordering::Release);

    // Ring buffer: ~0.5 seconds of audio.
    let ring_capacity = (info.sample_rate as usize) * (info.channels as usize);
    let ring_buf = Arc::new(RingBuffer::new(ring_capacity));
    let stop_flag = Arc::new(AtomicBool::new(false));
    let samples_written = Arc::new(AtomicU64::new(0));

    let volume = VolumeControl::new(0.5, info.sample_rate);

    // Build cpal output stream.
    let config = StreamConfig {
        channels: info.channels,
        sample_rate: cpal::SampleRate(info.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let ring_ref = Arc::clone(&ring_buf);
    let stop_ref = Arc::clone(&stop_flag);
    let paused_ref = Arc::clone(is_paused);

    let stream = match device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if paused_ref.load(Ordering::Acquire)
                || stop_ref.load(Ordering::Acquire)
            {
                data.fill(0.0);
                return;
            }
            let read = ring_ref.read(data);
            // Fill remainder with silence if ring buffer underrun.
            data[read..].fill(0.0);
        },
        move |err| {
            tracing::error!("cpal stream error: {err}");
        },
        None,
    ) {
        Ok(s) => s,
        Err(e) => {
            let _ = event_tx.send(AudioEvent::Error(format!(
                "Failed to build output stream: {e}"
            )));
            return EngineState::Idle;
        }
    };

    if let Err(e) = stream.play() {
        let _ = event_tx
            .send(AudioEvent::Error(format!("Failed to start stream: {e}")));
        return EngineState::Idle;
    }

    let _ = event_tx.send(AudioEvent::Playing {
        path: path.to_path_buf(),
        duration: info.duration,
        sample_rate: info.sample_rate,
        channels: info.channels,
    });

    EngineState::Playing {
        ring_buf,
        decoder,
        volume,
        stop_flag,
        samples_written,
        channels: info.channels,
        sample_rate: info.sample_rate,
        _stream: stream,
    }
}

/// Compute frequency spectrum bars from time-domain samples using FFT.
fn compute_spectrum(
    samples: &[f32],
    fft: &std::sync::Arc<dyn rustfft::Fft<f32>>,
    sample_rate: u32,
    num_bars: usize,
) -> Vec<f32> {
    use rustfft::num_complex::Complex;

    let n = samples.len();

    // Apply Hann window and create complex input.
    let mut input: Vec<Complex<f32>> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos());
            Complex::new(s * window, 0.0)
        })
        .collect();

    fft.process(&mut input);

    // Compute magnitudes for positive frequencies only.
    let half = n / 2;
    let magnitudes: Vec<f32> = input[..half].iter().map(|c| c.norm()).collect();

    // Group into bars using logarithmic frequency scale (like CAVA).
    let min_freq = 50.0_f32;
    let max_freq = (sample_rate as f32 / 2.0).min(16000.0);
    let freq_per_bin = sample_rate as f32 / n as f32;

    let mut bars = vec![0.0_f32; num_bars];
    for (bar_idx, bar) in bars.iter_mut().enumerate() {
        let f_low = min_freq * (max_freq / min_freq).powf(bar_idx as f32 / num_bars as f32);
        let f_high =
            min_freq * (max_freq / min_freq).powf((bar_idx + 1) as f32 / num_bars as f32);

        let bin_low = (f_low / freq_per_bin) as usize;
        let bin_high = ((f_high / freq_per_bin) as usize).min(half - 1);

        let mut sum = 0.0;
        let mut count = 0;
        for bin in bin_low..=bin_high {
            if bin < half {
                sum += magnitudes[bin];
                count += 1;
            }
        }

        *bar = if count > 0 { sum / count as f32 } else { 0.0 };
    }

    // Convert to dB-like scale and normalize for display.
    for bar in &mut bars {
        // Add small epsilon to avoid log(0), convert to dB-ish scale.
        *bar = (20.0 * (*bar + 1e-10).log10()).max(-60.0);
        // Map -60dB..0dB → 0.0..1.0
        *bar = (*bar + 60.0) / 60.0;
        *bar = bar.clamp(0.0, 1.0);
    }

    bars
}
