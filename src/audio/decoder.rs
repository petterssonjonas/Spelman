use std::fs::File;
use std::path::Path;
use std::time::Duration;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use crate::util::error::SpelmanError;

/// Information about a decoded audio track.
#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration: Duration,
}

/// Wraps symphonia to decode audio files into PCM f32 samples.
pub struct AudioDecoder {
    reader: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    pub info: TrackInfo,
}

impl AudioDecoder {
    pub fn open(path: &Path) -> Result<Self, SpelmanError> {
        let file = File::open(path).map_err(SpelmanError::Io)?;
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
            .map_err(|e| SpelmanError::Decode(e.to_string()))?;

        let reader = probed.format;

        let track = reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or(SpelmanError::NoAudioTrack)?;

        let track_id = track.id;
        let codec_params = track.codec_params.clone();

        let sample_rate = codec_params
            .sample_rate
            .ok_or_else(|| SpelmanError::Decode("Unknown sample rate".into()))?;

        let channels = codec_params
            .channels
            .map(|c| c.count() as u16)
            .unwrap_or(2);

        let duration = codec_params
            .n_frames
            .map(|n| Duration::from_secs_f64(n as f64 / sample_rate as f64))
            .unwrap_or_default();

        let decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|e| SpelmanError::Decode(e.to_string()))?;

        Ok(Self {
            reader,
            decoder,
            track_id,
            info: TrackInfo {
                sample_rate,
                channels,
                duration,
            },
        })
    }

    /// Decode the next chunk of audio. Returns interleaved f32 samples,
    /// or None if the stream is finished.
    pub fn next_samples(&mut self) -> Result<Option<Vec<f32>>, SpelmanError> {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(SpelmanError::Decode(e.to_string())),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                Err(e) => return Err(SpelmanError::Decode(e.to_string())),
            };

            let spec = *decoded.spec();
            let num_frames = decoded.frames();
            let _num_channels = spec.channels.count();

            let mut sample_buf = SampleBuffer::<f32>::new(
                num_frames as u64,
                spec,
            );
            sample_buf.copy_interleaved_ref(decoded);

            return Ok(Some(sample_buf.samples().to_vec()));
        }
    }

    /// Seek to a position in the stream.
    pub fn seek(&mut self, position: Duration) -> Result<(), SpelmanError> {
        let time = Time::from(position.as_secs_f64());
        self.reader
            .seek(SeekMode::Coarse, SeekTo::Time { time, track_id: Some(self.track_id) })
            .map_err(|e| SpelmanError::Decode(e.to_string()))?;
        self.decoder.reset();
        Ok(())
    }
}
