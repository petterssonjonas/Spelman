use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{Receiver, bounded};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::ui::widgets::visualizer::BarStyle;

/// Waveform display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveformMode {
    /// Classic bottom-up amplitude bars.
    Classic,
    /// Oscilloscope — symmetric spread from a center line.
    Oscilloscope,
}

impl Default for WaveformMode {
    fn default() -> Self {
        Self::Classic
    }
}

impl WaveformMode {
    pub fn next(self) -> Self {
        match self {
            Self::Classic => Self::Oscilloscope,
            Self::Oscilloscope => Self::Classic,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::Oscilloscope => "Oscilloscope",
        }
    }
}

/// Pre-scanned waveform data: normalized peak amplitudes (0.0–1.0) per bucket.
#[derive(Clone)]
pub struct WaveformData {
    /// One peak value per bucket (typically 1 bucket per column width).
    pub peaks: Vec<f32>,
    /// The track this waveform belongs to.
    pub path: PathBuf,
}

/// Manages background waveform scanning and caches the result.
pub struct WaveformState {
    /// The currently loaded waveform (if any).
    pub data: Option<WaveformData>,
    /// Receiver for the background scan result.
    rx: Option<Receiver<WaveformData>>,
    /// Cancel flag for the current scan.
    cancel: Arc<AtomicBool>,
}

impl Default for WaveformState {
    fn default() -> Self {
        Self {
            data: None,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl WaveformState {
    /// Start scanning a track for waveform data in the background.
    /// Cancels any previous scan. Does not block.
    pub fn scan(&mut self, path: &Path) {
        // If we already have data for this path, skip.
        if let Some(ref data) = self.data {
            if data.path == path {
                return;
            }
        }

        // Cancel any in-flight scan.
        self.cancel.store(true, Ordering::Release);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = cancel.clone();

        let path = path.to_path_buf();
        let (tx, rx) = bounded(1);
        self.rx = Some(rx);

        thread::Builder::new()
            .name("waveform-scan".into())
            .spawn(move || {
                if cancel.load(Ordering::Acquire) {
                    return;
                }
                if let Some(peaks) = scan_waveform(&path, &cancel) {
                    let _ = tx.send(WaveformData {
                        peaks,
                        path,
                    });
                }
            })
            .expect("Failed to spawn waveform scanner thread");
    }

    /// Poll for completed scan results. Non-blocking.
    pub fn poll(&mut self) {
        let rx = match self.rx.take() {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok(data) => {
                self.data = Some(data);
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {
                self.rx = Some(rx);
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => {}
        }
    }

    /// Clear cached waveform (e.g. on stop).
    pub fn clear(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.data = None;
        self.rx = None;
    }
}

/// Scan an audio file and produce peak amplitudes bucketed into ~2000 bins.
/// Returns None on error or cancellation.
fn scan_waveform(path: &Path, cancel: &AtomicBool) -> Option<Vec<f32>> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .ok()?;

    let mut reader = probed.format;

    let track = reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)?;

    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let sample_rate = codec_params.sample_rate?;
    let channels = codec_params.channels.map(|c| c.count()).unwrap_or(2);
    let n_frames = codec_params.n_frames.unwrap_or(sample_rate as u64 * 300);

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .ok()?;

    // Target ~2000 buckets regardless of track length.
    const TARGET_BUCKETS: usize = 2000;
    let frames_per_bucket = (n_frames as usize / TARGET_BUCKETS).max(1);
    let samples_per_bucket = frames_per_bucket * channels;

    let mut peaks: Vec<f32> = Vec::with_capacity(TARGET_BUCKETS + 1);
    let mut bucket_peak: f32 = 0.0;
    let mut bucket_count: usize = 0;
    let mut packets_decoded: u32 = 0;

    loop {
        if packets_decoded % 64 == 0 && cancel.load(Ordering::Acquire) {
            return None;
        }

        let packet = match reader.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(_) => break,
        };

        let spec = *decoded.spec();
        let num_frames = decoded.frames();
        let mut sample_buf = SampleBuffer::<f32>::new(num_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        for &s in sample_buf.samples() {
            let abs = s.abs();
            if abs > bucket_peak {
                bucket_peak = abs;
            }
            bucket_count += 1;

            if bucket_count >= samples_per_bucket {
                peaks.push(bucket_peak);
                bucket_peak = 0.0;
                bucket_count = 0;
            }
        }

        packets_decoded += 1;
    }

    if bucket_count > 0 {
        peaks.push(bucket_peak);
    }

    if peaks.is_empty() {
        return None;
    }

    // Normalize to max peak, then apply a power curve to expand dynamic range.
    // Without this, quiet sections look nearly as tall as loud ones because
    // peak normalization compresses the range.
    let max = peaks.iter().cloned().fold(0.0_f32, f32::max);
    if max > 0.0 {
        for p in &mut peaks {
            // Normalize to 0.0–1.0.
            *p /= max;
            // Power curve (sqrt-ish): expands differences between quiet and loud.
            // 0.6 exponent gives ~40% more dynamic range than linear.
            *p = p.powf(0.75);
        }
    }

    Some(peaks)
}

// ── Braille waveform renderer ───────────────────────────────────────────────

/// Braille base codepoint (U+2800).
const BRAILLE_BASE: u32 = 0x2800;

/// Braille dot bit positions — each cell is a 2-wide x 4-tall dot grid.
/// Left column bits:  [0]=row0, [1]=row1, [2]=row2, [6]=row3
/// Right column bits: [3]=row0, [4]=row1, [5]=row2, [7]=row3
const LEFT_DOTS: [u8; 4] = [0x01, 0x02, 0x04, 0x40];
const RIGHT_DOTS: [u8; 4] = [0x08, 0x10, 0x20, 0x80];

/// Compute a waveform color based on the bar style theme.
/// `height_frac` is 0.0 (bottom/center) to 1.0 (top/edge) within the waveform.
/// `played` indicates whether this column is in the played portion.
fn waveform_color(bar_style: BarStyle, height_frac: f32, played: bool) -> Color {
    if !played {
        return Color::Rgb(60, 60, 70);
    }
    match bar_style {
        BarStyle::Cyan => {
            // Brighter cyan near the base, dimmer toward the top.
            let t = 1.0 - height_frac;
            Color::Rgb(
                (20.0 + 30.0 * height_frac) as u8,
                (140.0 + 115.0 * t) as u8,
                (180.0 + 75.0 * t) as u8,
            )
        }
        BarStyle::Gradient => {
            // Cyan-teal-blue gradient from base to edge.
            let t = height_frac;
            if t < 0.5 {
                let u = t / 0.5;
                Color::Rgb(
                    (10.0 + 20.0 * u) as u8,
                    (200.0 - 60.0 * u) as u8,
                    (240.0 - 40.0 * u) as u8,
                )
            } else {
                let u = (t - 0.5) / 0.5;
                Color::Rgb(
                    (30.0 + 20.0 * u) as u8,
                    (140.0 - 60.0 * u) as u8,
                    (200.0 - 80.0 * u) as u8,
                )
            }
        }
        BarStyle::Spectrum => {
            // Green → yellow → red from base to edge.
            let t = height_frac;
            if t < 0.33 {
                let u = t / 0.33;
                Color::Rgb(
                    (40.0 * u) as u8,
                    (180.0 + 40.0 * (1.0 - u)) as u8,
                    30,
                )
            } else if t < 0.66 {
                let u = (t - 0.33) / 0.33;
                Color::Rgb(
                    (40.0 + 180.0 * u) as u8,
                    (180.0 - 10.0 * u) as u8,
                    20,
                )
            } else {
                let u = (t - 0.66) / 0.34;
                Color::Rgb(
                    (220.0 + 20.0 * u) as u8,
                    (170.0 - 130.0 * u) as u8,
                    10,
                )
            }
        }
    }
}

/// Renders a waveform as braille dots below the seek bar (classic bottom-up).
pub struct Waveform<'a> {
    /// Pre-scanned peak data.
    pub peaks: &'a [f32],
    /// Current playback fraction (0.0–1.0) for coloring played vs unplayed.
    pub fraction: f64,
    /// Bar style for color theming.
    pub bar_style: BarStyle,
}

impl<'a> Widget for Waveform<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 1 || self.peaks.is_empty() {
            return;
        }

        let cols = area.width as usize;
        let rows = area.height as usize;

        // Each braille cell covers 2 horizontal samples and 4 vertical dots.
        // Total vertical resolution = rows * 4 dots.
        let v_res = rows * 4;

        // Resample peaks: we need 2 samples per column (left dot + right dot).
        let h_samples = cols * 2;
        let resampled = resample_peaks(self.peaks, h_samples);

        // Build braille grid: cols x rows of dot bitmasks.
        // Also track max dot height per column for gradient coloring.
        let mut grid = vec![0u8; cols * rows];
        let mut col_heights = vec![0usize; cols];

        for (sample_idx, &peak) in resampled.iter().enumerate() {
            let col = sample_idx / 2;
            let is_right = sample_idx % 2 == 1;

            if col >= cols {
                break;
            }

            // Height in dots (from the bottom, going up).
            let dot_height = (peak * v_res as f32).round() as usize;
            if dot_height > col_heights[col] {
                col_heights[col] = dot_height;
            }

            let dots = if is_right { &RIGHT_DOTS } else { &LEFT_DOTS };

            for dot in 0..dot_height.min(v_res) {
                let row_from_bottom = dot / 4;
                let dot_in_row = dot % 4;
                let braille_dot_idx = 3 - dot_in_row;

                let row = rows - 1 - row_from_bottom;
                grid[row * cols + col] |= dots[braille_dot_idx];
            }
        }

        // Render braille characters to the buffer with themed colors.
        for row in 0..rows {
            let y = area.y + row as u16;
            for col in 0..cols {
                let bits = grid[row * cols + col];
                if bits == 0 {
                    continue;
                }

                let ch = char::from_u32(BRAILLE_BASE + bits as u32).unwrap_or(' ');

                let col_frac = (col as f64 + 0.5) / cols as f64;
                let played = col_frac <= self.fraction;

                // Compute height fraction for this cell row relative to the
                // column's peak height — gives a vertical gradient effect.
                let cell_from_bottom = (rows - 1 - row) as f32;
                let max_h = (col_heights[col] as f32 / 4.0).max(1.0);
                let height_frac = (cell_from_bottom / max_h).clamp(0.0, 1.0);

                let color = waveform_color(self.bar_style, height_frac, played);

                let x = area.x + col as u16;
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(color));
                }
            }
        }
    }
}

/// Renders a waveform as a center-spreading oscilloscope using braille dots.
/// When silent, a single center line. When loud, spreads symmetrically.
pub struct WaveformOscilloscope<'a> {
    /// Pre-scanned peak data.
    pub peaks: &'a [f32],
    /// Current playback fraction (0.0–1.0) for coloring played vs unplayed.
    pub fraction: f64,
    /// Bar style for color theming.
    pub bar_style: BarStyle,
}

impl<'a> Widget for WaveformOscilloscope<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 1 || self.peaks.is_empty() {
            return;
        }

        let cols = area.width as usize;
        let rows = area.height as usize;
        let v_res = rows * 4; // total dot rows
        let center = v_res / 2;

        // Resample peaks: 2 samples per column.
        let h_samples = cols * 2;
        let resampled = resample_peaks(self.peaks, h_samples);

        // Build braille grid with symmetric center-spread.
        let mut grid = vec![0u8; cols * rows];
        let mut col_disps = vec![0usize; cols]; // max displacement per column

        for (sample_idx, &peak) in resampled.iter().enumerate() {
            let col = sample_idx / 2;
            let is_right = sample_idx % 2 == 1;

            if col >= cols {
                break;
            }

            // Displacement from center (0.0 → 0 dots, 1.0 → center dots).
            let disp = (peak * center as f32).round() as usize;
            if disp > col_disps[col] {
                col_disps[col] = disp;
            }

            let dots = if is_right { &RIGHT_DOTS } else { &LEFT_DOTS };

            // Light dots from (center - disp) to (center + disp).
            let top_dot = center.saturating_sub(disp);
            let bot_dot = (center + disp).min(v_res - 1);

            for dot in top_dot..=bot_dot {
                let cell_row = dot / 4;
                let dot_in_cell = dot % 4;
                if cell_row < rows {
                    grid[cell_row * cols + col] |= dots[dot_in_cell];
                }
            }
        }

        // Render braille characters.
        for row in 0..rows {
            let y = area.y + row as u16;
            for col in 0..cols {
                let bits = grid[row * cols + col];
                if bits == 0 {
                    continue;
                }

                let ch = char::from_u32(BRAILLE_BASE + bits as u32).unwrap_or(' ');

                let col_frac = (col as f64 + 0.5) / cols as f64;
                let played = col_frac <= self.fraction;

                // Height fraction: distance from center for gradient.
                let cell_center_dist = ((row as f32 + 0.5) * 4.0 - center as f32).abs();
                let max_disp = (col_disps[col] as f32).max(1.0);
                let height_frac = (cell_center_dist / max_disp).clamp(0.0, 1.0);

                let color = waveform_color(self.bar_style, height_frac, played);

                let x = area.x + col as u16;
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(color));
                }
            }
        }

        // Draw center line in empty columns (silence baseline).
        let center_cell_row = center / 4;
        let center_dot_in_cell = center % 4;
        if center_cell_row < rows {
            let cy = area.y + center_cell_row as u16;
            for col in 0..cols {
                let x = area.x + col as u16;
                if let Some(cell) = buf.cell_mut((x, cy)) {
                    if cell.symbol() == " " {
                        let pattern = LEFT_DOTS[center_dot_in_cell]
                            | RIGHT_DOTS[center_dot_in_cell];
                        let ch = char::from_u32(BRAILLE_BASE + pattern as u32).unwrap_or(' ');
                        let col_frac = (col as f64 + 0.5) / cols as f64;
                        let played = col_frac <= self.fraction;
                        let color = if played {
                            Color::Rgb(40, 90, 110)
                        } else {
                            Color::Rgb(35, 40, 45)
                        };
                        cell.set_symbol(&ch.to_string());
                        cell.set_fg(color);
                    }
                }
            }
        }
    }
}

/// Resample peaks to target count using max-pooling within each bucket.
fn resample_peaks(peaks: &[f32], target: usize) -> Vec<f32> {
    if peaks.is_empty() || target == 0 {
        return vec![0.0; target];
    }

    let mut result = Vec::with_capacity(target);
    let src_len = peaks.len() as f64;

    for i in 0..target {
        let start = (i as f64 * src_len / target as f64) as usize;
        let end = (((i + 1) as f64 * src_len / target as f64) as usize).max(start + 1);
        let end = end.min(peaks.len());

        let max_val = peaks[start..end]
            .iter()
            .cloned()
            .fold(0.0_f32, f32::max);
        result.push(max_val);
    }

    result
}
