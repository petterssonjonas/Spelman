use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, Rgba};
use lofty::file::TaggedFileExt;
use lofty::picture::PictureType;
use std::path::Path;

/// Terminal graphics capability.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GraphicsProtocol {
    /// Kitty graphics protocol (kitty, Rio, WezTerm).
    Kitty,
    /// No image support — use ASCII art.
    Ascii,
}

/// Detect the terminal's graphics capability.
pub fn detect_protocol() -> GraphicsProtocol {
    // Check TERM_PROGRAM for known Kitty-compatible terminals.
    if let Ok(term) = std::env::var("TERM_PROGRAM") {
        match term.to_lowercase().as_str() {
            "wezterm" => return GraphicsProtocol::Kitty,
            _ => {}
        }
    }

    // Check TERM for kitty.
    if let Ok(term) = std::env::var("TERM") {
        if term.contains("kitty") {
            return GraphicsProtocol::Kitty;
        }
    }

    // Check KITTY_PID as a fallback.
    if std::env::var("KITTY_PID").is_ok() {
        return GraphicsProtocol::Kitty;
    }

    GraphicsProtocol::Ascii
}

/// Extract cover art from an audio file.
pub fn extract_cover(path: &Path) -> Option<Vec<u8>> {
    let tagged = lofty::probe::Probe::open(path)
        .ok()?
        .guess_file_type()
        .ok()?
        .read()
        .ok()?;

    let tag = tagged.primary_tag().or(tagged.first_tag())?;

    // Try front cover first, then any picture.
    let picture = tag
        .pictures()
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| tag.pictures().first())?;

    Some(picture.data().to_vec())
}

/// Load image data into a DynamicImage.
pub fn load_image(data: &[u8]) -> Option<DynamicImage> {
    image::load_from_memory(data).ok()
}

/// Render album art as ASCII/block art for terminals without image support.
/// Returns a Vec of Strings, one per line, using Unicode half-block characters
/// to display 2 vertical pixels per character cell.
pub fn render_ascii(img: &DynamicImage, width: u32, height: u32) -> Vec<String> {
    // Each character cell shows 2 pixels vertically using half-blocks.
    let pixel_height = height * 2;
    let resized = img.resize_exact(width, pixel_height, FilterType::Lanczos3);

    let mut lines = Vec::new();

    for y in (0..pixel_height).step_by(2) {
        let mut line = String::new();
        for x in 0..width {
            let top = resized.get_pixel(x, y);
            let bottom = if y + 1 < pixel_height {
                resized.get_pixel(x, y + 1)
            } else {
                Rgba([0, 0, 0, 0])
            };

            // Use upper half block: foreground = top pixel, background = bottom pixel.
            line.push_str(&format!(
                "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m\u{2580}",
                top[0], top[1], top[2], bottom[0], bottom[1], bottom[2]
            ));
        }
        line.push_str("\x1b[0m");
        lines.push(line);
    }

    lines
}

/// Render album art using Kitty graphics protocol.
/// Returns the escape sequence string to display the image.
pub fn render_kitty(img: &DynamicImage, width: u32, height: u32) -> String {
    let resized = img.resize(width * 8, height * 16, FilterType::Lanczos3);
    let rgba = resized.to_rgba8();
    let raw_data = rgba.as_raw();
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, raw_data);

    let (img_w, img_h) = resized.dimensions();

    // Kitty protocol: send image in chunks of 4096 bytes.
    let mut result = String::new();
    let chunks: Vec<&str> = encoded
        .as_bytes()
        .chunks(4096)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();

    for (i, chunk) in chunks.iter().enumerate() {
        let more = if i + 1 < chunks.len() { 1 } else { 0 };
        if i == 0 {
            // First chunk: include image parameters.
            result.push_str(&format!(
                "\x1b_Gf=32,s={img_w},v={img_h},a=T,t=d,m={more};{chunk}\x1b\\"
            ));
        } else {
            result.push_str(&format!("\x1b_Gm={more};{chunk}\x1b\\"));
        }
    }

    result
}

/// Cached album art for the current track.
#[derive(Debug, Clone)]
pub struct AlbumArt {
    /// The path of the track this art belongs to.
    pub track_path: Option<std::path::PathBuf>,
    /// Pre-rendered ASCII lines (for ASCII mode).
    pub ascii_lines: Vec<String>,
    /// Whether art was found.
    pub has_art: bool,
}

impl Default for AlbumArt {
    fn default() -> Self {
        Self {
            track_path: None,
            ascii_lines: Vec::new(),
            has_art: false,
        }
    }
}

impl AlbumArt {
    /// Update the cached art for a new track.
    pub fn update(&mut self, path: &Path, protocol: GraphicsProtocol, width: u32, height: u32) {
        if self.track_path.as_deref() == Some(path) {
            return; // Already cached for this track.
        }

        self.track_path = Some(path.to_path_buf());
        self.ascii_lines.clear();
        self.has_art = false;

        if let Some(data) = extract_cover(path) {
            if let Some(img) = load_image(&data) {
                self.has_art = true;
                match protocol {
                    GraphicsProtocol::Kitty => {
                        // Kitty rendering is done directly to stdout, not cached as lines.
                        // We'll just flag that art exists.
                    }
                    GraphicsProtocol::Ascii => {
                        self.ascii_lines = render_ascii(&img, width, height);
                    }
                }
            }
        }
    }
}
