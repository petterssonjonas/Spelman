//! Terminal image protocol detection and rendering.
//!
//! Supports:
//! - **Kitty graphics protocol** — native in Kitty, also supported by WezTerm, Ghostty
//! - **iTerm2 inline images** — supported by iTerm2, WezTerm, Mintty, Hyper
//! - **Fallback** — half-block Unicode rendering (handled elsewhere)
//!
//! Detection runs once at startup and caches the result.

use base64::Engine as _;
use std::io::Write;
use std::sync::OnceLock;

/// Detected image protocol capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    /// Kitty graphics protocol (transmit PNG, place at cursor).
    Kitty,
    /// iTerm2 inline image protocol (base64 PNG in OSC 1337).
    Iterm2,
    /// No image protocol — use half-block fallback.
    None,
}

static DETECTED: OnceLock<ImageProtocol> = OnceLock::new();

/// Detect the terminal's image protocol support.  Cached after first call.
pub fn detect() -> ImageProtocol {
    *DETECTED.get_or_init(detect_inner)
}

fn detect_inner() -> ImageProtocol {
    let term = std::env::var("TERM").unwrap_or_default();
    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    let kitty_pid = std::env::var("KITTY_PID").is_ok();
    let tmux = std::env::var("TMUX").is_ok();

    tracing::debug!(
        "Image protocol detection: TERM={term:?}, TERM_PROGRAM={term_program:?}, KITTY_PID={kitty_pid}, TMUX={tmux}"
    );

    // Inside tmux, check if the *outer* terminal supports image protocols.
    // Modern tmux (3.3+) can pass through Kitty graphics with `allow-passthrough`.
    // We detect the outer terminal via TERM_PROGRAM (set by outer, inherited through tmux).
    if tmux {
        // KITTY_PID is inherited through tmux when running inside Kitty.
        if kitty_pid || term_program.to_lowercase().contains("kitty") {
            tracing::info!("Image protocol: Kitty detected through tmux (passthrough)");
            return ImageProtocol::Kitty;
        }
        let tp_lower = term_program.to_lowercase();
        if tp_lower.contains("wezterm") || tp_lower.contains("ghostty") {
            tracing::info!("Image protocol: Kitty-compatible detected through tmux");
            return ImageProtocol::Kitty;
        }
        if tp_lower.contains("iterm") {
            tracing::info!("Image protocol: iTerm2 detected through tmux");
            return ImageProtocol::Iterm2;
        }
        tracing::info!("Image protocol: disabled inside tmux (outer terminal unknown)");
        return ImageProtocol::None;
    }

    if term.contains("kitty")
        || std::env::var("KITTY_PID").is_ok()
    {
        tracing::info!("Image protocol: Kitty (TERM={term:?}, KITTY_PID)");
        return ImageProtocol::Kitty;
    }

    // WezTerm and Ghostty support Kitty protocol.
    let tp_lower = term_program.to_lowercase();
    if tp_lower.contains("wezterm") || tp_lower.contains("ghostty") {
        tracing::info!("Image protocol: Kitty-compatible (TERM_PROGRAM={term_program:?})");
        return ImageProtocol::Kitty;
    }

    // iTerm2: TERM_PROGRAM=iTerm.app or LC_TERMINAL=iTerm2.
    if tp_lower.contains("iterm")
        || std::env::var("LC_TERMINAL")
            .map(|v| v.to_lowercase().contains("iterm"))
            .unwrap_or(false)
    {
        tracing::info!("Image protocol: iTerm2");
        return ImageProtocol::Iterm2;
    }

    tracing::info!("Image protocol: None (no supported protocol detected)");
    ImageProtocol::None
}

/// Encode a DynamicImage as PNG bytes in memory.
pub fn encode_png(img: &image::DynamicImage) -> Option<Vec<u8>> {
    use std::io::Cursor;
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(buf.into_inner())
}

/// Render an image using the Kitty graphics protocol.
///
/// Places the image at terminal position (`col`, `row`) spanning
/// `cols` columns and `rows` rows.  The image is transmitted as PNG
/// and the terminal handles scaling.
pub fn render_kitty(
    out: &mut impl Write,
    png_data: &[u8],
    col: u16,
    row: u16,
    cols: u16,
    rows: u16,
) -> std::io::Result<()> {
    // Move cursor to target position.
    write!(out, "\x1b[{};{}H", row + 1, col + 1)?;

    // Kitty protocol: transmit PNG in chunks of 4096 base64 chars.
    // Format: ESC_G <key=value,...> ; <base64 data> ESC \
    //
    // Keys:
    //   a=T    — action: transmit and display
    //   f=100  — format: PNG
    //   t=d    — transmission: direct (inline data)
    //   c=N    — display width in columns
    //   r=N    — display height in rows
    //   m=1/0  — more data follows (chunked transfer)
    //   q=2    — suppress responses (avoid stdin pollution)
    let b64 = base64::engine::general_purpose::STANDARD.encode(png_data);
    let chunks: Vec<&str> = b64.as_bytes().chunks(4096).map(|c| {
        std::str::from_utf8(c).unwrap_or("")
    }).collect();

    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i == chunks.len() - 1;
        if i == 0 {
            // First chunk: include all control keys.
            write!(
                out,
                "\x1b_Ga=T,f=100,t=d,c={},r={},q=2,m={};{}\x1b\\",
                cols,
                rows,
                if is_last { 0 } else { 1 },
                chunk,
            )?;
        } else {
            // Continuation chunk.
            write!(
                out,
                "\x1b_Gm={};{}\x1b\\",
                if is_last { 0 } else { 1 },
                chunk,
            )?;
        }
    }

    Ok(())
}

/// Render an image using the iTerm2 inline image protocol.
///
/// Places the image at terminal position (`col`, `row`) spanning
/// `cols` columns and `rows` rows.
pub fn render_iterm2(
    out: &mut impl Write,
    png_data: &[u8],
    col: u16,
    row: u16,
    cols: u16,
    rows: u16,
) -> std::io::Result<()> {
    // Move cursor to target position.
    write!(out, "\x1b[{};{}H", row + 1, col + 1)?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(png_data);

    // iTerm2 protocol:
    //   OSC 1337 ; File=<args> : <base64 data> ST
    //
    // Args:
    //   inline=1          — display inline (not download)
    //   width=Ncells      — display width in cells
    //   height=Ncells     — display height in cells
    //   preserveAspectRatio=1
    write!(
        out,
        "\x1b]1337;File=inline=1;width={cols};height={rows};preserveAspectRatio=1:{b64}\x07",
    )?;

    Ok(())
}

/// Erase a previously placed Kitty image (delete all placements).
/// Call this before rendering a new image to avoid ghosting.
pub fn kitty_clear(out: &mut impl Write) -> std::io::Result<()> {
    // Delete all images: a=d, d=A (all placements, all images).
    write!(out, "\x1b_Ga=d,d=A,q=2\x1b\\")?;
    Ok(())
}
