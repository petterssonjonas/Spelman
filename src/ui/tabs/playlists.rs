use crate::playlist::playlist::Playlist;

/// Saved playlists shared by the home tab and playlist picker.
#[derive(Debug, Clone, Default)]
pub struct PlaylistsState {
    pub playlists: Vec<Playlist>,
}

impl PlaylistsState {
    /// Reload playlists from disk.
    pub fn reload(&mut self) {
        self.playlists = crate::playlist::playlist::PlaylistManager::load_all();
    }
}
