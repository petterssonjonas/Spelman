use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use cpal::traits::HostTrait;
use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::audio::bridge::{self, EngineState};
use crate::util::channels::{AudioCommand, AudioEvent};

// ── AudioEngine (public API) ──────────────────────────────────────────────────

/// The audio engine manages decoding and playback.
pub struct AudioEngine {
    cmd_tx: Sender<AudioCommand>,
    event_rx: Receiver<AudioEvent>,
    handle: Option<thread::JoinHandle<()>>,
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
            handle: Some(handle),
        }
    }

    pub fn send(&self, cmd: AudioCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn event_rx(&self) -> &Receiver<AudioEvent> {
        &self.event_rx
    }

    /// Shut down the audio engine, joining the background thread.
    pub fn shutdown(&mut self) {
        // Drop the sender so the engine thread's recv() returns Err and exits.
        // We need to replace cmd_tx with a dummy to drop it.
        let (_dummy_tx, _) = unbounded::<AudioCommand>();
        let old_tx = std::mem::replace(&mut self.cmd_tx, _dummy_tx);
        drop(old_tx);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        // If shutdown() wasn't called explicitly, join here.
        if let Some(handle) = self.handle.take() {
            // The cmd_tx is about to be dropped, which will unblock the engine thread.
            // We can't join here because cmd_tx hasn't been dropped yet — the thread
            // might be blocked on recv. Just let the thread detach naturally.
            drop(handle);
        }
    }
}

// ── engine_thread ─────────────────────────────────────────────────────────────

fn engine_thread(cmd_rx: Receiver<AudioCommand>, event_tx: Sender<AudioEvent>) {
    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            let _ = event_tx
                .send(AudioEvent::Error("No audio output device found".into()));
            return;
        }
    };

    let mut state = EngineState::Idle;
    let is_paused = Arc::new(AtomicBool::new(false));
    let mut last_pos_samples: u64 = 0;
    let mut current_volume: f32 = 0.5;

    loop {
        // ── Idle: block on the next command ──────────────────────────────
        if matches!(state, EngineState::Idle) {
            last_pos_samples = 0;
            match cmd_rx.recv() {
                Ok(AudioCommand::Play(path)) => {
                    state = bridge::start_playback(
                        &path,
                        &device,
                        &event_tx,
                        &is_paused,
                        current_volume,
                    );
                }
                Ok(AudioCommand::SetVolume(v)) => {
                    current_volume = v;
                }
                Ok(_) => {}
                Err(_) => return,
            }
            continue;
        }

        // ── Playing: process commands (non-blocking) ─────────────────────
        let mut new_state: Option<EngineState> = None;

        if let EngineState::Playing {
            decoder,
            dsp,
            stop_flag,
            seek_pending,
            samples_played,
            sample_rate,
            channels,
            ..
        } = &mut state
        {
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    AudioCommand::Play(path) => {
                        stop_flag.store(true, Ordering::Release);
                        last_pos_samples = 0;
                        new_state = Some(bridge::start_playback(
                            &path,
                            &device,
                            &event_tx,
                            &is_paused,
                            current_volume,
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
                        } else {
                            seek_pending.store(true, Ordering::Release);
                            let new_pos_samples = (pos.as_secs_f64()
                                * *sample_rate as f64
                                * *channels as f64)
                                as u64;
                            samples_played
                                .store(new_pos_samples, Ordering::Release);
                            last_pos_samples = new_pos_samples;
                        }
                    }
                    AudioCommand::SetVolume(v) => {
                        current_volume = v;
                        dsp.volume.set_volume(v);
                    }
                    AudioCommand::SetEq(gains) => {
                        dsp.eq.set_all_gains(gains);
                    }
                    AudioCommand::ToggleEq => {
                        dsp.eq.set_enabled(!dsp.eq.enabled());
                    }
                }
            }
        }

        if let Some(ns) = new_state {
            state = ns;
            continue;
        }

        // ── Decode → DSP → ring buffer ──────────────────────────────────
        if let EngineState::Playing {
            producer,
            decoder,
            dsp,
            samples_played,
            channels,
            sample_rate,
            ..
        } = &mut state
        {
            if producer.slots() > 4096 {
                match decoder.next_samples() {
                    Ok(Some(mut samples)) => {
                        // Run the full DSP chain (volume, RMS, spectrum).
                        dsp.process(&mut samples, &event_tx);

                        // Push processed samples into the SPSC ring buffer.
                        let to_push = samples.len().min(producer.slots());
                        for &s in &samples[..to_push] {
                            let _ = producer.push(s);
                        }

                        // Throttle Position events: ~10 Hz.
                        let played = samples_played.load(Ordering::Relaxed);
                        let threshold =
                            (*sample_rate as u64 * *channels as u64) / 10;
                        if played.saturating_sub(last_pos_samples) >= threshold {
                            last_pos_samples = played;
                            let frames = played / *channels as u64;
                            let pos = Duration::from_secs_f64(
                                frames as f64 / *sample_rate as f64,
                            );
                            let _ = event_tx.send(AudioEvent::Position(pos));
                        }
                    }
                    Ok(None) => {
                        // Track finished — wait for ring buffer to drain.
                        drain_and_finish(
                            &mut state,
                            &cmd_rx,
                            &event_tx,
                            &device,
                            &is_paused,
                            &mut last_pos_samples,
                            current_volume,
                        );
                    }
                    Err(e) => {
                        let _ = event_tx
                            .send(AudioEvent::Error(e.to_string()));
                        if let EngineState::Playing { stop_flag, .. } =
                            &mut state
                        {
                            stop_flag.store(true, Ordering::Release);
                        }
                        state = EngineState::Idle;
                    }
                }
            } else {
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

/// Wait for the ring buffer to drain after the decoder reports EOF,
/// while remaining responsive to Stop/Play commands.
fn drain_and_finish(
    state: &mut EngineState,
    cmd_rx: &Receiver<AudioCommand>,
    event_tx: &Sender<AudioEvent>,
    device: &cpal::Device,
    is_paused: &Arc<AtomicBool>,
    last_pos_samples: &mut u64,
    current_volume: f32,
) {
    if let EngineState::Playing {
        producer,
        stop_flag,
        ..
    } = state
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if producer.slots() == producer.buffer().capacity() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                tracing::warn!("drain_and_finish timed out after 5s");
                break;
            }
            if let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    AudioCommand::Stop => {
                        stop_flag.store(true, Ordering::Release);
                        let _ = event_tx.send(AudioEvent::Stopped);
                        *state = EngineState::Idle;
                        return;
                    }
                    AudioCommand::Play(path) => {
                        stop_flag.store(true, Ordering::Release);
                        *last_pos_samples = 0;
                        *state = bridge::start_playback(
                            &path,
                            device,
                            event_tx,
                            is_paused,
                            current_volume,
                        );
                        return;
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
                    _ => {}
                }
            } else {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    // Only emit Finished if we didn't transition via Stop/Play above.
    if let EngineState::Playing { stop_flag, .. } = state {
        stop_flag.store(true, Ordering::Release);
        let _ = event_tx.send(AudioEvent::Finished);
    }
    *state = EngineState::Idle;
}
