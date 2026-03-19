# Spelman — Future Ideas

Ideas for features and improvements. Remove items as they are implemented.

## Audio

- **Gapless playback** — pre-decode the next track's first samples while the current one drains, so there's no silence gap between tracks. The rtrb + drain_and_finish architecture is already set up for this. - Toggle on or off in settings.

- **Audio normalization (ReplayGain)** — read ReplayGain tags from metadata and apply per-track gain adjustment in the DSP chain, so tracks from different albums play at consistent volume. - Toggle on or off in settings.

- **Crossfade** — fade out the last N seconds of a track while fading in the next one. Needs a second decoder running in parallel, mixing into the ring buffer. - Toggle on or off in settings.

## UI / Visual

- **Volume bar blocks** — replace the volume indicator with blocks that increase in size and color intensity as volume increases. - The percentage shows at the end too. 

- **Clickable playback controls** — make the Play/Pause icon and the "Playing" tab label mouse-clickable to toggle playback. The album art should be clickable to play/pause as well.

- **Better track metadata display** — show artist, album, and song title as separate styled lines on the Playing tab, reading from file metadata. These lines should have shimmer. 

- Shimmer effect toggle in settings, as well as shimmer intensity and speed.

- **Narrower seek bar** — make the seek bar (with current time and total time) 80% of the terminal width instead of full width. Also thicker bar.

- **Waveform overview** — pre-scan the track to generate a full waveform thumbnail, render it behind the seek bar so you can visually see where the loud/quiet parts are. 

- **Lyrics display** — parse embedded lyrics from tags (USLT/SYLT) or .lrc sidecar files, display synced lyrics on the Playing tab scrolling in time with playback. Can be toggled on of off and replace album art.

- **CAVA integration** — integrate [cava](https://github.com/karlstav/cava) as an audio visualizer option alongside the built-in FFT spectrum. Could pipe audio data to cava or use its raw output mode to render bars in the TUI.

- **Chroma visualizer overlay** — integrate [chroma](https://github.com/Shahid-Shabbir/chroma) as a toggleable visual overlay (default keybind: `C`). Renders chromatic/color effects over the app UI while music plays.

- **Clickable "Keybindings reference" link** — make the "Keybindings reference: K" text on the Home tab mouse-clickable to open the keybindings popup. 

- **Glimmer/shimmer effect on seek bar** — extend the traveling brightness wave to also shimmer across the seek bar in addition to song/artist/album text.

- Debug the album art and its interaction with the mouse. there seems to be something behind it, or in it that the mouse selects.

## System Integration

- **Global media key support** — listen for MPRIS (Linux) / MediaPlayer (macOS) system media keys so hardware play/pause/next buttons work even when the terminal isn't focused. Volume keys too.

- Notify-send and similar integrations, toggleable on and off in settings. song name when it starts.

## AI / Ollama

- **Mood-based queue generation** — connect to local Ollama to generate playlists based on mood descriptions, time of day, or activity context. 

- **Smart auto-DJ** — analyze listening patterns and automatically queue similar tracks when the current queue runs out. Creates playlists that holds the similar tracks. 

- **Natural language search** — "play something chill" or "that song from yesterday" using Ollama to interpret intent.

- Discuss if AI tab should be shared with text llm and ace-step, or separate tabs.

## AI Music Generation

- **ACE-Step integration** — research and integrate [ACE-Step](https://github.com/ace-step/ACE-Step) AI music generator for generating music directly within the player.

- **BYOK AI music services** — bring-your-own-key support for external AI music generation services (Suno, Udio, etc.), allowing users to generate tracks via their own API keys.

## Vindharpa GUI frontend

- Spelman should be a backend for Vindharpa, a frontend GUI music player.

- Spelman should be able to hang in the background as a service, and Vindharpa, or scripts can control it. Spelman --service perhaps.


