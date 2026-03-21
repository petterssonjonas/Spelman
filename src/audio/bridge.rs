use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::StreamConfig;
use crossbeam_channel::Sender;

use crate::audio::decoder::AudioDecoder;
use crate::audio::pipeline::{DspChain, SpectrumAnalyser};
use crate::util::channels::AudioEvent;

/// The state of the engine, either idle or actively playing.
pub enum EngineState {
    Idle,
    Playing {
        /// SPSC producer — decode thread writes here.
        producer: rtrb::Producer<f32>,
        decoder: AudioDecoder,
        dsp: DspChain,
        stop_flag: Arc<AtomicBool>,
        /// Set true after a seek so the cpal callback discards stale samples.
        seek_pending: Arc<AtomicBool>,
        /// Incremented by the cpal callback; used for position tracking.
        samples_played: Arc<AtomicU64>,
        channels: u16,
        sample_rate: u32,
        /// Keeps the stream alive.
        _stream: cpal::Stream,
        /// SPSC consumer — receives copies of played-back samples for spectrum.
        spec_consumer: rtrb::Consumer<f32>,
        /// Spectrum analyser — fed from the output side for accurate sync.
        analyser: SpectrumAnalyser,
    },
}

/// Set up a cpal output stream for the given audio file and return the
/// engine state. Returns `EngineState::Idle` on failure (errors are sent
/// through `event_tx`).
pub fn start_playback(
    path: &Path,
    device: &cpal::Device,
    event_tx: &Sender<AudioEvent>,
    is_paused: &Arc<AtomicBool>,
    current_volume: f32,
    volume_atomic: &Arc<AtomicU32>,
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

    // ~0.5 seconds of audio in the SPSC ring buffer.
    let ring_capacity =
        (info.sample_rate as usize) * (info.channels as usize);
    let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(ring_capacity);

    // Spectrum capture ring — the cpal callback copies played samples here
    // so the engine thread can compute FFT from what the listener actually
    // hears, not from pre-buffered decoded audio.
    let (mut spec_producer, spec_consumer) =
        rtrb::RingBuffer::<f32>::new(32768);

    let analyser = SpectrumAnalyser::new(info.sample_rate);

    let stop_flag = Arc::new(AtomicBool::new(false));
    let seek_pending = Arc::new(AtomicBool::new(false));
    let samples_played = Arc::new(AtomicU64::new(0));

    let dsp = DspChain::new(info.sample_rate, info.channels, current_volume);

    let config = StreamConfig {
        channels: info.channels,
        sample_rate: cpal::SampleRate(info.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let stop_ref = Arc::clone(&stop_flag);
    let seek_ref = Arc::clone(&seek_pending);
    let played_ref = Arc::clone(&samples_played);
    let paused_ref = Arc::clone(is_paused);
    let err_tx = event_tx.clone();
    let vol_ref = Arc::clone(volume_atomic);
    let mut callback_vol: f32 = current_volume;
    let vol_ramp: f32 = 1.0 / (info.sample_rate as f32 * 0.005);

    let stream = match device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if paused_ref.load(Ordering::Acquire)
                || stop_ref.load(Ordering::Acquire)
            {
                data.fill(0.0);
                return;
            }

            // If a seek just happened, drain all stale buffered samples.
            if seek_ref.load(Ordering::Acquire) {
                let stale = consumer.slots();
                if stale > 0 {
                    if let Ok(chunk) = consumer.read_chunk(stale) {
                        chunk.commit_all();
                    }
                }
                seek_ref.store(false, Ordering::Release);
                data.fill(0.0);
                return;
            }

            // Bulk-read from ring into output — single atomic commit.
            let available = consumer.slots();
            let to_read = data.len().min(available);
            let filled = if to_read > 0 {
                match consumer.read_chunk(to_read) {
                    Ok(chunk) => {
                        let n = {
                            let (first, second) = chunk.as_slices();
                            data[..first.len()].copy_from_slice(first);
                            if !second.is_empty() {
                                let split = first.len();
                                data[split..split + second.len()]
                                    .copy_from_slice(second);
                            }
                            first.len() + second.len()
                        };
                        chunk.commit_all();
                        n
                    }
                    Err(_) => 0,
                }
            } else {
                0
            };
            // Silence any underrun tail.
            data[filled..].fill(0.0);
            played_ref.fetch_add(filled as u64, Ordering::Relaxed);

            // Bulk-copy played samples to spectrum ring — single atomic commit.
            // Done pre-volume so spectrum shows spectral content, not loudness.
            if filled > 0 {
                let spec_avail = spec_producer.slots();
                let spec_write = filled.min(spec_avail);
                if spec_write > 0 {
                    if let Ok(chunk) =
                        spec_producer.write_chunk_uninit(spec_write)
                    {
                        chunk.fill_from_iter(
                            data[..spec_write].iter().copied(),
                        );
                    }
                }
            }

            // Apply volume with smooth ramping — post-ring for instant response.
            let target = f32::from_bits(vol_ref.load(Ordering::Relaxed));
            for s in &mut data[..filled] {
                if (callback_vol - target).abs() > vol_ramp {
                    callback_vol += if callback_vol < target {
                        vol_ramp
                    } else {
                        -vol_ramp
                    };
                } else {
                    callback_vol = target;
                }
                *s *= callback_vol;
            }
        },
        move |err| {
            let _ = err_tx
                .send(AudioEvent::Error(format!("cpal stream error: {err}")));
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
        let _ = event_tx.send(AudioEvent::Error(format!(
            "Failed to start stream: {e}"
        )));
        return EngineState::Idle;
    }

    let _ = event_tx.send(AudioEvent::Playing {
        path: path.to_path_buf(),
        duration: info.duration,
        sample_rate: info.sample_rate,
        channels: info.channels,
    });

    EngineState::Playing {
        producer,
        decoder,
        dsp,
        stop_flag,
        seek_pending,
        samples_played,
        channels: info.channels,
        sample_rate: info.sample_rate,
        _stream: stream,
        spec_consumer,
        analyser,
    }
}
