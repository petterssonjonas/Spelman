# Rust Engineer Review — 2026-03-19

## Priority 1 — Safety / Correctness

- [x] **P1-A** Integer underflow panic in `ProgressBar::render` — saturating_sub guard
- [x] **P1-B** Unchecked indexing into `EQ_PRESETS` — defensive `.get()` + empty guard
- [x] **P1-C** Infinite busy-wait in `drain_and_finish` — 5s timeout deadline
- [x] **P1-D** Rapid track-skip spawns unbounded meta-loader threads — cancellation flag via `AtomicBool`
- [x] **P1-E** Hardcoded `visible = 20` in scroll — uses actual `content_rect` height

## Priority 2 — Performance

- [x] **P2-A** `Vec::drain(..FFT_SIZE)` O(n) from front — replaced with `VecDeque`
- [x] **P2-B** `all_albums()`/`all_tracks()` allocate in `item_count()` — uses iterator `.sum()` instead
- [x] **P2-C** Search clones all matching `Track` structs per keystroke — stores flat indices, no cloning
- [x] **P2-D** `to_lowercase()` allocates per dir entry — `eq_ignore_ascii_case` instead
- [x] **P2-E** `format!` alloc per tab per frame in hover — `name.len() + 2`
- [x] **P2-F** One `String` alloc per bar character in ProgressBar — builds strings once

## Priority 3 — Architecture / Design

- [x] **P3-A** Terminal not restored on panic — panic hook installed
- [x] **P3-B** `record_recent` O(n) on every frame — gated on track change
- [x] **P3-C** Playlist filename sanitization can collide — FNV hash suffix
- [x] **P3-D** Weak shuffle RNG — improved seed with heap address + XOR constant
- [ ] **P3-E** God-struct `App` with 27+ fields — future refactor
- [x] **P3-F** `format_duration` duplicated 3x — extracted to `util/format.rs`
- [x] **P3-G** EQ `MAX_CHANNELS=2` silently corrupts non-stereo — increased to 8

## Priority 4 — Rust Idioms

- [x] **P4-A** `!is_some()` → `is_none()`
- [x] **P4-B** `&PathBuf` → `&Path` in `record_recent`
- [x] **P4-C** Opaque `.unwrap()` in `string_to_key` — replaced with `.map()`
- [x] **P4-E** Dead `AiCoordinator` — removed from App and module tree
- [x] **P4-F** Unused `QueueSource` trait — removed
- [x] **P4-G** `Cargo.toml` edition 2024 — added `rust-version = "1.85"`

## Priority 5 — Resource Management

- [x] **P5-A** Audio engine thread not joined — `shutdown()` method + `Drop` impl
- [x] **P5-B** Library scan thread detached — `JoinHandle` stored in `scan_handle`
- [x] **P5-C** Unbounded metadata thread spawns — `AtomicBool` cancellation flag
- [x] **P5-D** `lofty` opens file twice per track — merged into single open
