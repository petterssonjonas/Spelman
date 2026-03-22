# Spelman — Future Ideas

Ideas for features and improvements. Remove items as they are implemented.

## Audio

- **Crossfade** — fade out the last N seconds of a track while fading in the next one. Needs a second decoder running in parallel, mixing into the ring buffer. - Toggle on or off in settings.

- Add ALAC, AIFF, mkv, caf, mpc feature flags to enable in symphonia

- Look at Opus support, opus or audiopus crate.

- WMA/ASF, APE, MPC are also not supported, look at those, 

## UI / Visual

- **Lyrics display** — parse embedded lyrics from tags (USLT/SYLT) or .lrc sidecar files, display synced lyrics on the Playing tab scrolling in time with playback. Can be toggled on of off and replace album art.

- **CAVA integration** — integrate [cava](https://github.com/karlstav/cava) as an audio visualizer option alongside the built-in FFT spectrum. Could pipe audio data to cava or use its raw output mode to render bars in the TUI.

- **Chroma visualizer overlay** — integrate [chroma](https://github.com/Shahid-Shabbir/chroma) as a toggleable visual overlay (default keybind: `C`). Renders chromatic/color effects over the app UI while music plays.

- **iTerm2 / Kitty image protocol for album art** — use iTerm2 inline image protocol and Kitty graphics protocol to render true-color album art in supported terminals, instead of half-block approximation. Auto-detect terminal support and fall back to current method.

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

## Streaming services integration

- Spotify - realistically only for 5 users.

- Youtube Music - with python api, but can be turned off at any time.

- Archive.org - free open api. Live shows, historical, niche mostly.

- Audiobooks - public domain books on archive.org. 

- Audius - The web3 alternative, decentralized, free, open no rate limits.
mostly independent electronic, hiphop, indie.

- Jamendo - public api, REST. Free for non commercial.

- Radio-Browser, global live radio. Free open api.

- Freesound - More lo-fi, ambient or soundscapes. Massive database. Have to attribute creators - good feature to build in. Very interesting.

- Unified Music APis like musicapi.com or odesli. One Api for spotify, tidal, deezer and audius. Generallt montly sub if more than a few users.

- freemusicarchive.org
royalty free muscic

## UI

Separate AI looking into integrating openstretmap tui world map for radio-browser.

