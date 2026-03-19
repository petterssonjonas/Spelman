use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::StreamConfig;
use crossbeam_channel::Sender;

use crate::audio::decoder::AudioDecoder;
use crate::audio::pipeline::DspChain;
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

    let stream = match device.build_output_stream(
        &config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            if paused_ref.load(Ordering::Acquire)
                || stop_ref.load(Ordering::Acquire)
            {
                data.fill(0.0);
                return;
            }

            // If a seek just happened, drain all stale buffered samples first.
            if seek_ref.load(Ordering::Acquire) {
                let available = consumer.slots();
                for _ in 0..available {
                    let _ = consumer.pop();
                }
                seek_ref.store(false, Ordering::Release);
                data.fill(0.0);
                return;
            }

            // Fill the output buffer from the ring; silence any underrun tail.
            let mut filled = 0_usize;
            for dst in data.iter_mut() {
                match consumer.pop() {
                    Ok(s) => {
                        *dst = s;
                        filled += 1;
                    }
                    Err(_) => {
                        *dst = 0.0;
                    }
                }
            }
            played_ref.fetch_add(filled as u64, Ordering::Relaxed);
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
    }
}
