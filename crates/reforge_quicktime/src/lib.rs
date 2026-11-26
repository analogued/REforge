// Copyright (C) 2026 The REforge contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Decode legacy QuickTime movies from seekable input and individual intra frames.

#![deny(unsafe_code)]

mod frames;

use std::{
    collections::BTreeMap,
    io::{self, Read, Seek, SeekFrom},
    time::Duration,
};

use nihav_core::{
    codecs::{NADecoder, NADecoderSupport, RegisteredDecoders},
    demuxers::{Demuxer, DemuxerError, RegisteredDemuxers, create_demuxer},
    formats::YUV420_FORMAT,
    frame::{NACodecInfo, NACodecTypeInfo, NAPacket, NAStream, NAVideoInfo, StreamType},
    io::byteio::{ByteIO, FileReader},
};

pub use frames::{AudioFrame, Frame, Plane, VideoFrame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoCodec {
    PhotoJpeg,
    Cinepak,
}

impl VideoCodec {
    fn name(self) -> &'static str {
        match self {
            Self::PhotoJpeg => "jpeg",
            Self::Cinepak => "cinepak",
        }
    }
}

/// Decode an independently coded video sample from an external sample table.
pub fn decode_still(
    codec: VideoCodec,
    width: u16,
    height: u16,
    bytes: Vec<u8>,
) -> io::Result<VideoFrame> {
    let (width, height) = (usize::from(width), usize::from(height));
    let info = NACodecInfo::new(
        codec.name(),
        NACodecTypeInfo::Video(NAVideoInfo::new(width, height, false, YUV420_FORMAT)),
        None,
    );
    let stream = NAStream::new(StreamType::Video, 0, info, 1, 1, 0).into_ref();
    let time = stream.make_ts(None, None, None);
    let packet = NAPacket::new(stream.clone(), time, true, bytes);
    let mut track = TrackDecoder::new(&registered_decoders(), &stream)?;
    let image = frames::video(&track.decode(&packet)?.get_buffer())?;
    if (image.luma.width, image.luma.height) != (width, height) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{} still dimensions disagree with sample table",
                codec.name()
            ),
        ));
    }
    Ok(image)
}

/// A movie stream whose byte offsets are relative to the start of its input.
pub struct MovieSource<R: Read + Seek> {
    reader: FileReader<R>,
}

impl<R: Read + Seek> MovieSource<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: FileReader::new_read(reader),
        }
    }

    /// Open a fresh decoder, starting at the beginning of the input.
    pub fn decoder(&mut self) -> io::Result<MovieDecoder<'_>> {
        self.reader
            .seek(SeekFrom::Start(0))
            .map_err(|error| io::Error::other(format!("rewind QuickTime input: {error:?}")))?;

        let mut demuxers = RegisteredDemuxers::new();
        nihav_commonfmt::generic_register_all_demuxers(&mut demuxers);
        let mov = demuxers
            .find_demuxer("mov")
            .ok_or_else(|| io::Error::other("QuickTime demuxer is unavailable"))?;
        let demuxer =
            create_demuxer(mov, &mut self.reader).map_err(|error| demux_error("open", error))?;
        let decoders = registered_decoders();
        let tracks = demuxer
            .get_streams()
            .filter(|stream| {
                matches!(
                    stream.get_media_type(),
                    StreamType::Video | StreamType::Audio
                )
            })
            .map(|stream| Ok((stream.get_id(), TrackDecoder::new(&decoders, &stream)?)))
            .collect::<io::Result<BTreeMap<_, _>>>()?;
        if tracks.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "QuickTime input has no audio or video tracks",
            ));
        }
        Ok(MovieDecoder { demuxer, tracks })
    }
}

pub struct MovieDecoder<'a> {
    demuxer: Demuxer<'a>,
    tracks: BTreeMap<u32, TrackDecoder>,
}

/// Decoded sample timing as reported by the movie demuxer.
#[derive(Debug)]
pub struct TimedFrame {
    pub track_id: u32,
    pub presentation_time: Option<Duration>,
    pub duration: Option<Duration>,
    pub frame: Frame,
}

impl MovieDecoder<'_> {
    /// Decode the next audio or video sample in demux order.
    ///
    /// Text and metadata tracks are outside the audio/video decoder's scope.
    pub fn next_frame(&mut self) -> io::Result<Option<TimedFrame>> {
        loop {
            let packet = match self.demuxer.get_frame() {
                Ok(packet) => packet,
                Err(DemuxerError::EOF) => return Ok(None),
                Err(error) => return Err(demux_error("read packet", error)),
            };
            let track_id = packet.get_stream().get_id();
            let Some(track) = self.tracks.get_mut(&track_id) else {
                continue;
            };
            let decoded = track.decode(&packet)?;
            let time = packet.get_time_information();
            let buffer = decoded.get_buffer();
            let frame = match packet.get_stream().get_media_type() {
                StreamType::Video => Frame::Video(frames::video(&buffer)?),
                StreamType::Audio => Frame::Audio(frames::audio(&buffer)?),
                _ => unreachable!("only audio and video have registered track decoders"),
            };
            let timestamp = |ticks| ticks_to_duration(ticks, time.tb_num, time.tb_den);
            let duration = match &frame {
                Frame::Audio(audio) => Some(audio.duration()),
                Frame::Video(_) => time.duration.and_then(timestamp),
            };
            return Ok(Some(TimedFrame {
                track_id,
                presentation_time: time.pts.or(time.dts).and_then(timestamp),
                duration,
                frame,
            }));
        }
    }
}

fn ticks_to_duration(ticks: u64, numerator: u32, denominator: u32) -> Option<Duration> {
    let ns = (u128::from(ticks) * u128::from(numerator) * 1_000_000_000)
        .checked_div(u128::from(denominator))?;
    let secs = u64::try_from(ns / 1_000_000_000).ok()?;
    Some(Duration::new(secs, (ns % 1_000_000_000) as u32))
}

struct TrackDecoder {
    decoder: Box<dyn NADecoder + Send>,
    support: NADecoderSupport,
}

impl TrackDecoder {
    fn new(decoders: &RegisteredDecoders, stream: &NAStream) -> io::Result<Self> {
        let info = stream.get_info();
        let name = info.get_name();
        let make_decoder = decoders.find_decoder(name).ok_or_else(|| {
            track_error(stream, io::ErrorKind::Unsupported, "decoder is unavailable")
        })?;
        let mut decoder = make_decoder();
        let mut support = NADecoderSupport::new();
        decoder.init(&mut support, info).map_err(|error| {
            track_error(
                stream,
                decode_error_kind(error),
                &format!("could not initialize {name} decoder ({error:?})"),
            )
        })?;
        Ok(Self { decoder, support })
    }

    fn decode(&mut self, packet: &NAPacket) -> io::Result<nihav_core::frame::NAFrameRef> {
        self.decoder
            .decode(&mut self.support, packet)
            .map_err(|error| {
                track_error(
                    &packet.get_stream(),
                    decode_error_kind(error),
                    &format!(
                        "sample at {:?} failed to decode ({error:?})",
                        packet.get_pts()
                    ),
                )
            })
    }
}

fn registered_decoders() -> RegisteredDecoders {
    let mut decoders = RegisteredDecoders::new();
    nihav_commonfmt::generic_register_all_decoders(&mut decoders);
    nihav_qt::qt_register_all_decoders(&mut decoders);
    decoders
}

fn track_error(stream: &NAStream, kind: io::ErrorKind, message: &str) -> io::Error {
    io::Error::new(
        kind,
        format!(
            "QuickTime track {} ({}): {message}",
            stream.get_id(),
            stream.get_info().get_name()
        ),
    )
}

fn decode_error_kind(error: nihav_core::codecs::DecoderError) -> io::ErrorKind {
    use nihav_core::codecs::DecoderError;
    match error {
        DecoderError::NotImplemented => io::ErrorKind::Unsupported,
        DecoderError::ShortData => io::ErrorKind::UnexpectedEof,
        DecoderError::AllocError => io::ErrorKind::OutOfMemory,
        DecoderError::Bug => io::ErrorKind::Other,
        _ => io::ErrorKind::InvalidData,
    }
}

fn demux_error(action: &str, error: DemuxerError) -> io::Error {
    let kind = match error {
        DemuxerError::EOF => io::ErrorKind::UnexpectedEof,
        DemuxerError::NotImplemented | DemuxerError::NotPossible => io::ErrorKind::Unsupported,
        DemuxerError::MemoryError => io::ErrorKind::OutOfMemory,
        DemuxerError::IOError | DemuxerError::SeekError => io::ErrorKind::Other,
        _ => io::ErrorKind::InvalidData,
    };
    io::Error::new(kind, format!("QuickTime {action}: {error:?}"))
}

#[cfg(test)]
mod tests {
    use super::{
        NACodecInfo, NACodecTypeInfo, NAPacket, NAStream, NAVideoInfo, StreamType, TrackDecoder,
        VideoCodec, YUV420_FORMAT, decode_still, frames, registered_decoders,
    };

    fn jpeg_with_restarts(second_restart: u8) -> Vec<u8> {
        let mut image = vec![0xff, 0xd8, 0xff, 0xdb, 0x00, 0x43, 0x00];
        image.extend([1; 64]);
        image.extend([
            // Baseline grayscale, three 8x8 blocks.
            0xff,
            0xc0,
            0x00,
            0x0b,
            8,
            0,
            8,
            0,
            24,
            1,
            1,
            0x11,
            0,
            // Restart the DC predictor after each block.
            0xff,
            0xdd,
            0x00,
            0x04,
            0,
            1,
            // Use the default Huffman tables.
            0xff,
            0xda,
            0x00,
            0x08,
            1,
            1,
            0,
            0,
            63,
            0,
            // Each entropy segment encodes the same DC delta (+15) and EOB.
            0xbf,
            0x5f,
            0xff,
            0xd0,
            0xbf,
            0x5f,
            0xff,
            second_restart,
            0xbf,
            0x5f,
            0xff,
            0xd9,
        ]);
        image
    }

    #[test]
    fn jpeg_restart_markers_reset_prediction_and_validate_sequence() {
        let image = decode_still(VideoCodec::PhotoJpeg, 24, 8, jpeg_with_restarts(0xd1)).unwrap();
        assert!(image.chroma.is_none());
        let level = image.luma.pixels[0];
        assert!(level > 128);
        assert!(image.luma.pixels.iter().all(|&pixel| pixel == level));

        let error =
            decode_still(VideoCodec::PhotoJpeg, 24, 8, jpeg_with_restarts(0xd2)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn rpza_retains_unmodified_blocks_between_frames() {
        let info = NACodecInfo::new(
            "apple-video",
            NACodecTypeInfo::Video(NAVideoInfo::new(8, 4, false, YUV420_FORMAT)),
            None,
        );
        let stream = NAStream::new(StreamType::Video, 0, info, 1, 15, 0).into_ref();
        let mut decoder = TrackDecoder::new(&registered_decoders(), &stream).unwrap();
        let decode = |decoder: &mut TrackDecoder, bytes: &[u8]| {
            let time = stream.make_ts(None, None, None);
            let packet = NAPacket::new(stream.clone(), time, true, bytes.to_vec());
            frames::video(&decoder.decode(&packet).unwrap().get_buffer()).unwrap()
        };
        // The initial frame fills two adjacent blocks. The next sample
        // skips the first block and replaces the second with blue.
        let first = decode(
            &mut decoder,
            &[0xe1, 0, 0, 10, 0xa0, 0x7c, 0, 0xa0, 0x03, 0xe0],
        );
        let second = decode(&mut decoder, &[0xe1, 0, 0, 8, 0x80, 0xa0, 0, 0x1f]);
        assert_eq!(
            &first.luma.pixels[..8],
            &[77, 77, 77, 77, 149, 149, 149, 149]
        );
        assert_eq!(&second.luma.pixels[..8], &[77, 77, 77, 77, 29, 29, 29, 29]);
    }
}
