use ogg::{writing::PacketWriteEndInfo, PacketWriter};
use opus::{Application, Bitrate, Channels, Encoder};
use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use std::io::{Cursor, Write};
use std::sync::Arc;
use thiserror::Error;

const SAMPLE_RATE: u16 = 48000;
const FRAME_SIZE: usize = 960; // 20ms at 48kHz
const MAX_PACKET_SIZE: usize = 1275; // Maximum size of an Opus packet
const MINIMUM_FRAME_SIZE: usize = 480; // 10ms at 48kHz

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Opus error: {0}")]
    Opus(#[from] opus::Error),
    #[error("SoundFont error: {0}")]
    SoundFont(String),
    #[error("MIDI error: {0}")]
    Midi(String),
    #[error("WAV parsing error: {0}")]
    WavParsing(String),
}

#[derive(Debug)]
pub enum OpusBitrate {
    Auto,
    Max,
    Bits(i32),
}

#[derive(Debug)]
struct WavHeader {
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    data_start: usize,
}

fn parse_wav_header(data: &[u8]) -> Result<WavHeader, AudioError> {
    if data.len() < 44 {
        return Err(AudioError::WavParsing("WAV data too short".to_string()));
    }

    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(AudioError::WavParsing("Invalid WAV file".to_string()));
    }

    let channels = u16::from_le_bytes([data[22], data[23]]);
    let sample_rate = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
    let bits_per_sample = u16::from_le_bytes([data[34], data[35]]);

    let mut data_start = 12;
    while data_start + 8 < data.len() {
        let chunk_type = &data[data_start..data_start + 4];
        let chunk_size = u32::from_le_bytes([
            data[data_start + 4],
            data[data_start + 5],
            data[data_start + 6],
            data[data_start + 7],
        ]) as usize;

        if chunk_type == b"data" {
            data_start += 8;
            break;
        }

        data_start = data_start
            .checked_add(8 + chunk_size)
            .ok_or_else(|| AudioError::WavParsing("Invalid chunk size".to_string()))?;
    }

    if data_start >= data.len() {
        return Err(AudioError::WavParsing("No data chunk found".to_string()));
    }

    Ok(WavHeader {
        channels,
        sample_rate,
        bits_per_sample,
        data_start,
    })
}

pub fn render_midi_to_wav(
    soundfont_bytes: &[u8],
    midi_bytes: &[u8],
) -> Result<Vec<u8>, AudioError> {
    let mut sf2 = Cursor::new(soundfont_bytes);
    let sound_font =
        Arc::new(SoundFont::new(&mut sf2).map_err(|e| AudioError::SoundFont(e.to_string()))?);

    let mut mid = Cursor::new(midi_bytes);
    let midi_file = Arc::new(MidiFile::new(&mut mid).map_err(|e| AudioError::Midi(e.to_string()))?);

    let settings = SynthesizerSettings::new(SAMPLE_RATE as i32);
    let synthesizer = Synthesizer::new(&sound_font, &settings)
        .map_err(|e| AudioError::SoundFont(e.to_string()))?;
    let mut sequencer = MidiFileSequencer::new(synthesizer);

    sequencer.play(&midi_file, false);

    let sample_count = (SAMPLE_RATE as f64 * midi_file.get_length()) as usize;
    let mut left: Vec<f32> = Vec::with_capacity(sample_count);
    let mut right: Vec<f32> = Vec::with_capacity(sample_count);

    // Render audio in chunks to avoid excessive memory usage
    const CHUNK_SIZE: usize = 1024;
    let mut temp_left = vec![0.0; CHUNK_SIZE];
    let mut temp_right = vec![0.0; CHUNK_SIZE];

    while left.len() < sample_count {
        let remaining = sample_count - left.len();
        let current_chunk_size = std::cmp::min(CHUNK_SIZE, remaining);
        temp_left.resize(current_chunk_size, 0.0);
        temp_right.resize(current_chunk_size, 0.0);

        sequencer.render(&mut temp_left, &mut temp_right);

        left.extend_from_slice(&temp_left);
        right.extend_from_slice(&temp_right);
    }

    let mut wav_data = Vec::new();

    // Write WAV header
    wav_data.extend_from_slice(b"RIFF");
    write_u32(&mut wav_data, 36 + (sample_count * 4) as u32)?; // File size - 8
    wav_data.extend_from_slice(b"WAVE");

    // Write format chunk
    wav_data.extend_from_slice(b"fmt ");
    write_u32(&mut wav_data, 16)?; // Chunk size
    wav_data.extend_from_slice(&1u16.to_le_bytes()); // Audio format (PCM)
    wav_data.extend_from_slice(&2u16.to_le_bytes()); // Number of channels
    write_u32(&mut wav_data, SAMPLE_RATE as u32)?; // Sample rate
    write_u32(&mut wav_data, (SAMPLE_RATE as u32) * 4)?; // Byte rate
    wav_data.extend_from_slice(&4u16.to_le_bytes()); // Block align
    wav_data.extend_from_slice(&16u16.to_le_bytes()); // Bits per sample

    // Write data chunk header
    wav_data.extend_from_slice(b"data");
    write_u32(&mut wav_data, (sample_count * 4) as u32)?; // Chunk size

    // Convert f32 samples to i16 and write to WAV data
    for (l, r) in left.iter().zip(right.iter()) {
        let left_sample = (l.clamp(-1.0, 0.99999994) * 32768.0) as i16;
        let right_sample = (r.clamp(-1.0, 0.99999994) * 32768.0) as i16;
        wav_data.write_all(&left_sample.to_le_bytes())?;
        wav_data.write_all(&right_sample.to_le_bytes())?;
    }

    Ok(wav_data)
}

pub fn wav_to_opus_ogg(
    wav_data: &[u8],
    stereo: bool,
    bitrate: OpusBitrate,
) -> Result<Vec<u8>, AudioError> {
    let wav_header = parse_wav_header(wav_data)?;

    if wav_header.sample_rate != SAMPLE_RATE as u32 {
        return Err(AudioError::WavParsing(format!(
            "Unsupported sample rate. Expected {}, got {}",
            SAMPLE_RATE, wav_header.sample_rate
        )));
    }

    let pcm_data = &wav_data[wav_header.data_start..];
    let channel_count = wav_header.channels as usize;

    let channels = if stereo {
        Channels::Stereo
    } else {
        Channels::Mono
    };
    let mut encoder = Encoder::new(SAMPLE_RATE as u32, channels, Application::Audio)?;

    match bitrate {
        OpusBitrate::Auto => encoder.set_bitrate(Bitrate::Auto)?,
        OpusBitrate::Max => encoder.set_bitrate(Bitrate::Max)?,
        OpusBitrate::Bits(bits) => encoder.set_bitrate(Bitrate::Bits(bits))?,
    }
    // Convert PCM data to Vec<i16>, handling both mono and stereo
    let samples: Vec<i16> = match wav_header.bits_per_sample {
        16 => pcm_data
            .chunks_exact(2 * channel_count)
            .flat_map(|chunk| {
                chunk
                    .chunks_exact(2)
                    .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
                    .take(if stereo { 2 } else { 1 })
            })
            .collect(),
        8 => pcm_data
            .iter()
            .map(|&sample| ((sample as i16 - 128) << 8))
            .collect(),
        _ => {
            return Err(AudioError::WavParsing(format!(
                "Unsupported bit depth: {}",
                wav_header.bits_per_sample
            )))
        }
    };

    let mut ogg_output = Vec::new();
    let mut granule_position = 0u64;

    {
        let mut packet_writer = PacketWriter::new(Cursor::new(&mut ogg_output));

        // Write Opus header
        let opus_header = create_opus_header(channels, SAMPLE_RATE as u32);
        packet_writer.write_packet(
            opus_header,
            1, // Serial number
            PacketWriteEndInfo::EndPage,
            0, // Granule position
        )?;

        // Write Opus comment header
        let opus_comment = create_opus_comment();
        packet_writer.write_packet(
            opus_comment,
            1, // Serial number
            PacketWriteEndInfo::EndPage,
            0, // Granule position
        )?;

        // Encode audio data
        for chunk in samples.chunks(FRAME_SIZE * channels as usize) {
            let mut packet = vec![0u8; MAX_PACKET_SIZE];
            // Underlying C implementation of OPUS encoder
            // cannot deal with frames shorter then 10ms.
            // The only chunk that can be shorter is the last one.
            // We pad the last chunk up to the minimum length.
            // TODO: smart length-aware iteration to avoid short chunks
            let chunk = &(pad_chunk(chunk, channels as usize));
            let packet_len = encoder.encode(chunk, &mut packet)?;
            packet.truncate(packet_len);

            granule_position = granule_position.saturating_add(FRAME_SIZE as u64);

            packet_writer.write_packet(
                packet,
                1, // Serial number
                PacketWriteEndInfo::NormalPacket,
                granule_position,
            )?;
        }

        // Write end of stream
        packet_writer.write_packet(
            Vec::new(),
            1, // Serial number
            PacketWriteEndInfo::EndStream,
            granule_position,
        )?;
    }

    Ok(ogg_output)
}

fn pad_chunk(chunk: &[i16], channels: usize) -> Vec<i16> {
    let min_length = MINIMUM_FRAME_SIZE * channels;
    let padding_size = (min_length as i16) - (chunk.len() as i16);
    if padding_size < 1 {
        return chunk.to_vec();
    }
    let mut padded = chunk.to_vec();
    padded.extend(std::iter::repeat(0 as i16).take(padding_size as usize));
    padded
}

fn create_opus_header(channels: Channels, sample_rate: u32) -> Vec<u8> {
    let mut header = vec![
        b'O',
        b'p',
        b'u',
        b's',
        b'H',
        b'e',
        b'a',
        b'd', // Magic signature
        1,    // Version
        channels as u8,
        0,
        0, // Pre-skip (3840 samples or 80ms)
        sample_rate.to_le_bytes()[0],
        sample_rate.to_le_bytes()[1],
        sample_rate.to_le_bytes()[2],
        sample_rate.to_le_bytes()[3],
        0,
        0, // Output gain
        0, // Channel mapping family (0 for mono/stereo)
    ];

    // Set pre-skip value (3840 samples or 80ms)
    header[10] = 0x00;
    header[11] = 0x0F;

    header
}

fn create_opus_comment() -> Vec<u8> {
    let vendor_string = b"midirenderer";
    let mut comment = vec![
        b'O',
        b'p',
        b'u',
        b's',
        b'T',
        b'a',
        b'g',
        b's', // Magic signature
        (vendor_string.len() as u32).to_le_bytes()[0],
        (vendor_string.len() as u32).to_le_bytes()[1],
        (vendor_string.len() as u32).to_le_bytes()[2],
        (vendor_string.len() as u32).to_le_bytes()[3],
    ];
    comment.extend_from_slice(vendor_string);
    comment.extend_from_slice(&[0, 0, 0, 0]); // User comment list length

    comment
}

fn write_u32(output: &mut Vec<u8>, value: u32) -> Result<(), AudioError> {
    output.write_all(&value.to_le_bytes())?;
    Ok(())
}
