use thiserror::Error;

#[derive(Error, Debug)]
pub enum SpelmanError {
    #[error("Audio error: {0}")]
    Audio(String),

    #[error("Decode error: {0}")]
    Decode(String),

    #[error("No audio tracks found in file")]
    NoAudioTrack,

    #[error("Unsupported sample format")]
    UnsupportedFormat,

    #[error("Device error: {0}")]
    Device(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Metadata error: {0}")]
    Metadata(String),
}
