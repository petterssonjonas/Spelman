use std::path::PathBuf;

use crate::config::settings::{RepeatMode, ShuffleMode};

/// A play queue — an ordered list of tracks with a cursor.
#[derive(Debug, Clone, Default)]
pub struct Queue {
    tracks: Vec<PathBuf>,
    /// Index of the currently playing track (None if queue is empty or nothing selected).
    current: Option<usize>,
    /// Shuffle order — indices into tracks. Only used when shuffle is on.
    shuffle_order: Vec<usize>,
    /// Current position in shuffle_order.
    shuffle_pos: usize,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a track to the end of the queue.
    pub fn push(&mut self, path: PathBuf) {
        self.tracks.push(path);
        if self.current.is_none() {
            self.current = Some(0);
        }
    }

    /// Add multiple tracks to the end of the queue.
    pub fn extend(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        let was_empty = self.tracks.is_empty();
        self.tracks.extend(paths);
        if was_empty && !self.tracks.is_empty() {
            self.current = Some(0);
        }
    }

    /// Clear the queue.
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.current = None;
        self.shuffle_order.clear();
        self.shuffle_pos = 0;
    }

    /// Get the current track path.
    pub fn current_track(&self) -> Option<&PathBuf> {
        self.current.and_then(|i| self.tracks.get(i))
    }

    /// Get the current track index.
    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    /// Advance to the next track, respecting shuffle and repeat modes.
    pub fn next_with_mode(
        &mut self,
        shuffle: ShuffleMode,
        repeat: RepeatMode,
    ) -> Option<&PathBuf> {
        if self.tracks.is_empty() {
            return None;
        }

        match repeat {
            RepeatMode::One => {
                // Stay on the same track.
                return self.current.and_then(|i| self.tracks.get(i));
            }
            _ => {}
        }

        if shuffle == ShuffleMode::On {
            return self.next_shuffled(repeat);
        }

        // Sequential mode.
        if let Some(idx) = self.current {
            if idx + 1 < self.tracks.len() {
                self.current = Some(idx + 1);
                return self.tracks.get(idx + 1);
            } else if repeat == RepeatMode::All {
                self.current = Some(0);
                return self.tracks.first();
            }
        }
        None
    }

    fn next_shuffled(&mut self, repeat: RepeatMode) -> Option<&PathBuf> {
        // Build shuffle order if needed.
        if self.shuffle_order.len() != self.tracks.len() {
            self.rebuild_shuffle();
        }

        self.shuffle_pos += 1;
        if self.shuffle_pos < self.shuffle_order.len() {
            let idx = self.shuffle_order[self.shuffle_pos];
            self.current = Some(idx);
            self.tracks.get(idx)
        } else if repeat == RepeatMode::All {
            self.rebuild_shuffle();
            self.shuffle_pos = 0;
            let idx = self.shuffle_order[0];
            self.current = Some(idx);
            self.tracks.get(idx)
        } else {
            None
        }
    }

    fn rebuild_shuffle(&mut self) {
        self.shuffle_order = (0..self.tracks.len()).collect();
        // Simple Fisher-Yates using a basic LCG (no external dep needed).
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64;

        for i in (1..self.shuffle_order.len()).rev() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let j = (seed >> 33) as usize % (i + 1);
            self.shuffle_order.swap(i, j);
        }
    }

    /// Advance to the next track (simple, no mode awareness — for backwards compat).
    pub fn next(&mut self) -> Option<&PathBuf> {
        if let Some(idx) = self.current {
            if idx + 1 < self.tracks.len() {
                self.current = Some(idx + 1);
                return self.tracks.get(idx + 1);
            }
        }
        None
    }

    /// Go back to the previous track.
    pub fn prev(&mut self) -> Option<&PathBuf> {
        if let Some(idx) = self.current {
            if idx > 0 {
                self.current = Some(idx - 1);
                return self.tracks.get(idx - 1);
            }
        }
        None
    }

    /// Set the current index directly.
    pub fn set_current(&mut self, index: usize) -> Option<&PathBuf> {
        if index < self.tracks.len() {
            self.current = Some(index);
            self.tracks.get(index)
        } else {
            None
        }
    }

    /// Remove a track at the given index.
    pub fn remove(&mut self, index: usize) {
        if index >= self.tracks.len() {
            return;
        }
        self.tracks.remove(index);
        if self.tracks.is_empty() {
            self.current = None;
        } else if let Some(cur) = self.current {
            if index < cur {
                self.current = Some(cur - 1);
            } else if index == cur && cur >= self.tracks.len() {
                self.current = Some(self.tracks.len() - 1);
            }
        }
        // Invalidate shuffle order.
        self.shuffle_order.clear();
    }

    /// Get all tracks in the queue.
    pub fn tracks(&self) -> &[PathBuf] {
        &self.tracks
    }

    /// Number of tracks in the queue.
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }
}
