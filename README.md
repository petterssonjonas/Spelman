# Spelman

A nice GPU accelerated, AI enabled terminal music player written in Rust.

Generate lofi music as you work, to match your mood. Tell it to make it a bit more mellow, or faster or make it jazz... 

Allow it to be your Pomodoro timer perhaps.

Tell it your doing a 10 minute workout. Get something vigorous.

Ask it to pull the latest news pod perhaps.

## Features

- **Audio playback** — MP3, FLAC, OGG, Opus, WAV, AAC via symphonia (pure Rust decoding)
- **Low-latency audio** — cpal output with lock-free ring buffer, no allocations in the audio callback
- **Volume control** — smooth 5ms ramping to avoid clicks/pops
- **Metadata display** — title, artist, album via lofty
- **TUI** — ratatui-based interface with tab bar, progress bar, level meter
- **Vim-style keybindings** — `Space` play/pause, `h/l` seek, `+/-` volume, `q` quit

## Usage

```sh
# Play a file
spelman song.flac

# Open without a file (library mode — coming soon)
spelman
```

## Building

```sh
cargo build --release
```

Requires ALSA development libraries on Linux:
```sh
# Fedora
sudo dnf install alsa-lib-devel

# Ubuntu/Debian
sudo apt install libasound2-dev
```

## Architecture

```
┌─────────────────┐   crossbeam channels   ┌──────────────────┐
│   Main Thread    │◄──────────────────────►│  Audio Thread     │
│  (ratatui loop)  │   AudioCommand/Event   │  (cpal callback)  │
└────────┬─────────┘                        └──────────────────┘
         │                                          ▲
         │                                    Ring Buffer
         │                                          │
         │                                  ┌──────────────────┐
         │                                  │  Decode Thread    │
         │                                  │  (symphonia +DSP) │
         │                                  └──────────────────┘
         │
         ▼
┌──────────────────┐
│  Library Thread   │  (coming soon)
│  (scan + index)   │
└──────────────────┘
```

- **Main thread**: ratatui event loop, input handling, UI rendering (~60fps)
- **Audio engine thread**: symphonia decoding → volume → ring buffer, sends position/level events via crossbeam channels
- **cpal callback**: reads from lock-free ring buffer (real-time safe)

## Roadmap

### Phase 3 — Search, Settings, Album Art *(in progress)*
- Search tab — filter-as-you-type across artist/album/title
- Settings tab — live config editing, persist to TOML
- Album art — Kitty graphics protocol (kitty, Rio), ASCII art fallback (alacritty, etc.)
- Shuffle & repeat modes (sequential, shuffle, repeat-one, repeat-all)
- Theming (TOML-based, bundled Catppuccin/Gruvbox)
- Terminal capability detection at startup

### Phase 4 — Pomodoro Timer
- Pomodoro mode that controls play/pause automatically
- Work session plays your music, break swaps to a clock tick-tock track
- UI transforms during breaks: analog clock, hourglass/sand timer, or digital countdown
- Red indicator when break time is up
- Configurable work/break durations

### Phase 5 — Podcasts & RSS
- Subscribe to RSS/Atom feeds for podcasts and newscasts
- Download or stream episodes directly
- Podcast-specific UI (show notes, episode list, playback speed)
- Configurable feed list in settings

### Phase 6 — AI Music Generation
- AI tab — generate lofi/ambient music on the fly
- Natural language control ("make it more mellow", "jazz it up", "something vigorous")
- Built-in synth engine + Ollama for local inference
- Cloud BYOK (bring your own API key) for hosted models
- Mood-aware: "doing a 10 minute workout, give me something vigorous"

### Future
- Spectrum visualizer with Blackman-Harris FFT
- 10-band graphic equalizer
- EBU R128 loudness normalization
- Gapless playback with crossfade
- Audio device selection and hot-switching
- MPRIS2 media controls
- Chroma-like audio-reactive visual effects
- SQLite library index with FTS5 search

## Credits

Inspired by [kew](https://github.com/ravachol/kew) by ravachol. Spelman is a ground-up Rust rewrite, not a fork.

## License

MIT
