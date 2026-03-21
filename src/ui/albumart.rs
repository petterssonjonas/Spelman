use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView};
use lofty::file::TaggedFileExt;
use lofty::picture::PictureType;
use ratatui::style::Color;
use std::path::Path;

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

/// A single cell of album art with foreground and background colors.
#[derive(Debug, Clone, Copy)]
pub struct ArtCell {
    pub fg: Color,
    pub bg: Color,
}

/// Render album art as a grid of colored half-block cells using ratatui Colors.
/// Each cell uses the upper half-block character (▀) with fg=top pixel, bg=bottom pixel.
pub fn render_art(img: &DynamicImage, width: u32, height: u32) -> Vec<Vec<ArtCell>> {
    let pixel_height = height * 2;
    let resized = img.resize_exact(width, pixel_height, FilterType::Triangle);

    let mut rows = Vec::with_capacity(height as usize);

    for y in (0..pixel_height).step_by(2) {
        let mut row = Vec::with_capacity(width as usize);
        for x in 0..width {
            let top = resized.get_pixel(x, y);
            let bottom = if y + 1 < pixel_height {
                resized.get_pixel(x, y + 1)
            } else {
                image::Rgba([0, 0, 0, 0])
            };
            row.push(ArtCell {
                fg: Color::Rgb(top[0], top[1], top[2]),
                bg: Color::Rgb(bottom[0], bottom[1], bottom[2]),
            });
        }
        rows.push(row);
    }

    rows
}

/// Cached album art for the current track.
#[derive(Debug, Clone)]
pub struct AlbumArt {
    /// The path of the track this art belongs to.
    pub track_path: Option<std::path::PathBuf>,
    /// Pre-rendered grid of colored cells (for half-block fallback rendering).
    pub cells: Vec<Vec<ArtCell>>,
    /// Whether art was found.
    pub has_art: bool,
    /// Raw image bytes (PNG/JPEG) for image protocol rendering.
    /// Kept alongside cells so both paths are available.
    pub raw_image: Option<Vec<u8>>,
}

impl Default for AlbumArt {
    fn default() -> Self {
        Self {
            track_path: None,
            cells: Vec::new(),
            has_art: false,
            raw_image: None,
        }
    }
}

impl AlbumArt {}
