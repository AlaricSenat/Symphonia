// Symphonia
// Copyright (c) 2019-2026 The Project Symphonia Developers.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;

use symphonia_core::audio::Position as P;
use symphonia_core::audio::{Audio, Channels, Position};
use symphonia_core::errors::Error;
use symphonia_core::units::{Duration, Timestamp};

// Syntactic elements and their speakers, in bitstream order (ISO/IEC 14496-3:2001, Table 1.11).
fn elements(config: u32) -> &'static [(u32, &'static [Position])] {
    match config {
        1 => &[(0, &[P::FRONT_CENTER])],
        2 => &[(1, &[P::FRONT_LEFT, P::FRONT_RIGHT])],
        3 => &[(0, &[P::FRONT_CENTER]), (1, &[P::FRONT_LEFT, P::FRONT_RIGHT])],
        4 => &[
            (0, &[P::FRONT_CENTER]),
            (1, &[P::FRONT_LEFT, P::FRONT_RIGHT]),
            (0, &[P::REAR_CENTER]),
        ],
        5 => &[
            (0, &[P::FRONT_CENTER]),
            (1, &[P::FRONT_LEFT, P::FRONT_RIGHT]),
            (1, &[P::REAR_LEFT, P::REAR_RIGHT]),
        ],
        6 => &[
            (0, &[P::FRONT_CENTER]),
            (1, &[P::FRONT_LEFT, P::FRONT_RIGHT]),
            (1, &[P::REAR_LEFT, P::REAR_RIGHT]),
            (3, &[P::LFE1]),
        ],
        7 => &[
            (0, &[P::FRONT_CENTER]),
            (1, &[P::FRONT_LEFT_CENTER, P::FRONT_RIGHT_CENTER]),
            (1, &[P::FRONT_LEFT, P::FRONT_RIGHT]),
            (1, &[P::REAR_LEFT, P::REAR_RIGHT]),
            (3, &[P::LFE1]),
        ],
        _ => unreachable!(),
    }
}

fn lc_asc(config: u32) -> BitWriter {
    let mut asc = BitWriter::default();
    asc.put(2, 5); // audioObjectType = AAC LC
    asc.put(7, 4); // samplingFrequencyIndex = 22050 Hz
    asc.put(config, 4);
    asc.put(0, 3); // GASpecificConfig()
    asc
}

fn put_element(w: &mut BitWriter, id: u32, tag: u32, speakers: &[Position], active: Position) {
    w.put(id, 3);
    w.put(tag, 4);
    if id == 1 {
        w.put_bool(false); // common_window
    }
    for &speaker in speakers {
        put_ics(w, active.contains(speaker));
    }
}

#[test]
fn verify_lc_speaker_mapping() {
    for config in 1..=7 {
        for &active in elements(config).iter().flat_map(|(_, speakers)| *speakers) {
            let asc_decoder = new_decoder_asc(&lc_asc(config)).unwrap();
            let channels = asc_decoder.codec_params().channels.clone().unwrap();
            for mut dec in [asc_decoder, new_decoder(channels).unwrap()] {
                let mut w = BitWriter::default();
                for (tag, &(id, speakers)) in elements(config).iter().enumerate() {
                    put_element(&mut w, id, tag as u32, speakers, active);
                }
                decode(&mut dec, &finish(w)).unwrap();

                for &speaker in elements(config).iter().flat_map(|(_, speakers)| *speakers) {
                    let samples = dec.buf.plane_by_position(speaker).unwrap();
                    assert_eq!(samples.len(), 1024);
                    assert!(samples.iter().all(|sample| sample.is_finite()));
                    assert_eq!(
                        energy(samples) > 0.0,
                        speaker == active,
                        "config={config}, active={active:?}, speaker={speaker:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn verify_invalid_channel_pairs() {
    let mut dec = new_decoder_asc(&lc_asc(6)).unwrap();
    // A pair cannot span the center and front-left speakers, or start at the final channel.
    assert!(matches!(dec.set_pair(0, 0, true), Err(Error::DecodeError(_))));
    assert!(matches!(dec.set_pair(0, 5, true), Err(Error::DecodeError(_))));
    assert!(matches!(dec.set_pair(0, 6, false), Err(Error::DecodeError(_))));
    assert!(dec.pairs.is_empty());
}

#[derive(Default)]
struct BitWriter {
    buf: Vec<u8>,
    len: usize,
}

impl BitWriter {
    fn put(&mut self, value: u32, bits: u32) {
        for i in (0..bits).rev() {
            if self.len % 8 == 0 {
                self.buf.push(0);
            }
            if (value >> i) & 1 != 0 {
                *self.buf.last_mut().unwrap() |= 0x80 >> (self.len % 8);
            }
            self.len += 1;
        }
    }

    fn put_bool(&mut self, value: bool) {
        self.put(u32::from(value), 1);
    }

    /// Pad with zero bits to a byte boundary and return the bytes.
    fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

/// Write an individual_channel_stream() (Table 4.6) without a common window, with a single
/// long window.
///
/// If `noise` is false, max_sfb is zero and the channel is silent. Otherwise, the first 8
/// scalefactor bands are coded by perceptual noise substitution (4.6.13), such that the channel
/// carries a low frequency noise signal.
fn put_ics(w: &mut BitWriter, noise: bool) {
    w.put(100, 8); // global_gain
    w.put_bool(false); // ics_reserved_bit
    w.put(u32::from(ONLY_LONG_SEQUENCE), 2); // window_sequence
    w.put_bool(false); // window_shape
    if noise {
        w.put(8, 6); // max_sfb
        w.put_bool(false); // predictor_data_present
        // section_data(): one section of 8 bands coded with NOISE_HCB.
        w.put(13, 4);
        w.put(8, 5);
        // scale_factor_data(): the first noise energy is coded with 9 bits (offset 256), the
        // others are Huffman coded differences of zero (codeword '0').
        w.put(256 + 100, 9);
        for _ in 1..8 {
            w.put(0, 1);
        }
    }
    else {
        w.put(0, 6); // max_sfb
        w.put_bool(false); // predictor_data_present
    }
    w.put_bool(false); // pulse_data_present
    w.put_bool(false); // tns_data_present
    w.put_bool(false); // gain_control_data_present
}

/// Write ID_END and return the raw_data_block().
fn finish(mut w: BitWriter) -> Vec<u8> {
    w.put(7, 3); // ID_END
    w.into_bytes()
}

fn new_decoder_asc(asc: &BitWriter) -> Result<AacDecoder> {
    AacDecoder::try_new(
        AudioCodecParameters::new()
            .for_codec(CODEC_ID_AAC)
            .with_extra_data(asc.buf.clone().into_boxed_slice()),
        &AudioDecoderOptions::default(),
    )
}

fn decode(dec: &mut AacDecoder, data: &[u8]) -> Result<()> {
    dec.decode_ref(&PacketRef::new(0, Timestamp::new(0), Duration::new(1024), data)).map(|_| ())
}

fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|&s| f64::from(s) * f64::from(s)).sum()
}

fn new_decoder(channels: Channels) -> Result<AacDecoder> {
    AacDecoder::try_new(
        AudioCodecParameters::new()
            .for_codec(CODEC_ID_AAC)
            .with_sample_rate(22050)
            .with_channels(channels),
        &AudioDecoderOptions::default(),
    )
}

#[test]
fn verify_unsupported_layouts() {
    for channels in [
        Channels::None,
        Channels::Discrete(0),
        Channels::Discrete(6),
        Channels::Ambisonic(1),
        layouts::CHANNEL_LAYOUT_AAC_QUADRAPHONIC,
        layouts::CHANNEL_LAYOUT_AAC_6P1,
        layouts::CHANNEL_LAYOUT_7P1,
    ] {
        assert!(matches!(new_decoder(channels), Err(Error::Unsupported(_))));
    }
    // Preserve support for callers providing only a mono or stereo channel count.
    for count in [1, 2] {
        assert!(new_decoder(Channels::Discrete(count)).is_ok());
    }
}
