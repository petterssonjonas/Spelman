use std::path::PathBuf;

/// A play queue — an ordered list of tracks with a cursor.
#[derive(Debug, Clone, Default)]
pub struct Queue {
    tracks: Vec<PathBuf>,
    /// Index of the currently playing track (None if queue is empty or nothing selected).
    current: Option<usize>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a track to the end of the queue.
    pub fn push(&mut self, path: PathBuf) {
        self.tracks.push(path);
        // If this is the first track, set current to it.
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
    }

    /// Get the current track path.
    pub fn current_track(&self) -> Option<&PathBuf> {
        self.current.and_then(|i| self.tracks.get(i))
    }

    /// Get the current track index.
    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    /// Advance to the next track. Returns the new current track, or None if at the end.
    pub fn next(&mut self) -> Option<&PathBuf> {
        if let Some(idx) = self.current {
            if idx + 1 < self.tracks.len() {
                self.current = Some(idx + 1);
                return self.tracks.get(idx + 1);
            }
        }
        None
    }

    /// Go back to the previous track. Returns the new current track, or None if at the start.
    pub fn prev(&mut self) -> Option<&PathBuf> {
        if let Some(idx) = self.current {
            if idx > 0 {
                self.current = Some(idx - 1);
                return self.tracks.get(idx - 1);
            }
        }
        None
    }

    /// Set the current index directly (e.g., user clicked a track in the queue).
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
