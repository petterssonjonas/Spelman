# Spelman

A terminal music player written in Rust. Inspired by [kew](https://github.com/ravachol/kew).

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

## Planned

- Tab-based navigation (Library, Playlists, Search, Settings, AI Player)
- Album art rendering (Kitty/Sixel/halfblock)
- Spectrum visualizer with Blackman-Harris FFT
- 10-band graphic equalizer
- EBU R128 loudness normalization
- Gapless playback with crossfade
- Audio device selection and hot-switching
- MPRIS2 media controls
- Chroma-like audio-reactive visual effects
- AI music generation tab (built-in synth + Ollama + cloud BYOK)
- Theming (TOML-based, bundled Catppuccin/Gruvbox)
- SQLite library index with FTS5 search

## Credits

Inspired by [kew](https://github.com/ravachol/kew) by ravachol. Spelman is a ground-up Rust rewrite, not a fork.

## License

MIT
