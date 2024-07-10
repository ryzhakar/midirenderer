use ogg::{writing::PacketWriteEndInfo, PacketWriter};
use opus::{Application, Bitrate, Channels, Encoder};
use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use std::io::{Cursor, Write};
use std::sync::Arc;
use thiserror::Error;

const SAMPLE_RATE: i32 = 48000;
const FRAME_SIZE: usize = 960; // 20ms at 48kHz
const MAX_PACKET_SIZE: usize = 1275; // Maximum size of an Opus packet

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

pub enum OpusBitrate {
    Auto,
    Max,
    Bits(i32),
}

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

        data_start += 8 + chunk_size;
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
    // Load the SoundFont from bytes
    let mut sf2 = Cursor::new(soundfont_bytes);
    let sound_font =
        Arc::new(SoundFont::new(&mut sf2).map_err(|e| AudioError::SoundFont(e.to_string()))?);

    // Load the MIDI file from bytes
    let mut mid = Cursor::new(midi_bytes);
    let midi_file = Arc::new(MidiFile::new(&mut mid).map_err(|e| AudioError::Midi(e.to_string()))?);

    // Create the synthesizer and sequencer
    let settings = SynthesizerSettings::new(SAMPLE_RATE);
    let synthesizer = Synthesizer::new(&sound_font, &settings)
        .map_err(|e| AudioError::SoundFont(e.to_string()))?;
    let mut sequencer = MidiFileSequencer::new(synthesizer);

    // Prepare to play the MIDI file
    sequencer.play(&midi_file, false);

    // Calculate the total number of samples
    let sample_count = (SAMPLE_RATE as f64 * midi_file.get_length()) as usize;

    // Create buffers for left and right channels
    let mut left: Vec<f32> = vec![0.0; sample_count];
    let mut right: Vec<f32> = vec![0.0; sample_count];

    // Render the audio
    sequencer.render(&mut left, &mut right);

    // Prepare the WAV file data
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
        let left_sample = (l.clamp(-1.0, 1.0) * 32767.0) as i16;
        let right_sample = (r.clamp(-1.0, 1.0) * 32767.0) as i16;
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

    if wav_header.channels != 2
        || wav_header.sample_rate != SAMPLE_RATE as u32
        || wav_header.bits_per_sample != 16
    {
        return Err(AudioError::WavParsing("Unsupported WAV format".to_string()));
    }

    let pcm_data = &wav_data[wav_header.data_start..];
    let samples: Vec<f32> = if stereo {
        pcm_data
            .chunks_exact(4) // 2 bytes per sample, 2 channels
            .flat_map(|chunk| {
                let left = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
                let right = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 / 32768.0;
                vec![left, right]
            })
            .collect()
    } else {
        pcm_data
            .chunks_exact(4) // 2 bytes per sample, 2 channels
            .map(|chunk| {
                let left = i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0;
                let right = i16::from_le_bytes([chunk[2], chunk[3]]) as f32 / 32768.0;
                (left + right) / 2.0 // Convert stereo to mono by averaging
            })
            .collect()
    };

    // Create Opus encoder
    let channels = if stereo {
        Channels::Stereo
    } else {
        Channels::Mono
    };
    let mut encoder = Encoder::new(SAMPLE_RATE as u32, channels, Application::Audio)?;

    // Set bitrate
    match bitrate {
        OpusBitrate::Auto => encoder.set_bitrate(Bitrate::Auto)?,
        OpusBitrate::Max => encoder.set_bitrate(Bitrate::Max)?,
        OpusBitrate::Bits(bits) => encoder.set_bitrate(Bitrate::Bits(bits))?,
    }

    // Prepare OGG stream
    let mut ogg_output = Vec::new();
    let mut packet_writer = PacketWriter::new(Cursor::new(&mut ogg_output));

    // Write Opus header packets
    write_opus_header(&mut packet_writer, channels, SAMPLE_RATE as u32)?;
    write_opus_comment_header(&mut packet_writer)?;

    // Encode and write
    let channel_count = if stereo { 2 } else { 1 };
    let mut granule_position = 0u32;
    for chunk in samples.chunks(FRAME_SIZE * channel_count) {
        let mut packet = vec![0u8; MAX_PACKET_SIZE];
        let packet_len = encoder.encode_float(chunk, &mut packet)?;
        packet.truncate(packet_len);

        granule_position += FRAME_SIZE as u32;

        packet_writer.write_packet(
            packet,
            granule_position,
            PacketWriteEndInfo::NormalPacket,
            0, // timestamp
        )?;
    }

    // Finalize OGG stream
    packet_writer.write_packet(
        Vec::new(),
        granule_position,
        PacketWriteEndInfo::EndStream,
        0,
    )?;
    drop(packet_writer);

    Ok(ogg_output)
}

fn write_opus_header(
    packet_writer: &mut PacketWriter<Cursor<&mut Vec<u8>>>,
    channels: Channels,
    sample_rate: u32,
) -> Result<(), AudioError> {
    let header = vec![
        b'O',
        b'p',
        b'u',
        b's',
        b'H',
        b'e',
        b'a',
        b'd',
        1, // version
        channels as u8,
        0,
        0, // pre-skip
        (sample_rate & 0xFF) as u8,
        ((sample_rate >> 8) & 0xFF) as u8,
        ((sample_rate >> 16) & 0xFF) as u8,
        ((sample_rate >> 24) & 0xFF) as u8,
        0,
        0, // output gain
        0, // channel mapping family
    ];

    packet_writer.write_packet(header, 0, PacketWriteEndInfo::NormalPacket, 0)?;

    Ok(())
}

fn write_opus_comment_header(
    packet_writer: &mut PacketWriter<Cursor<&mut Vec<u8>>>,
) -> Result<(), AudioError> {
    let comment = b"ENCODER=midirenderer";
    let mut header = vec![
        b'O',
        b'p',
        b'u',
        b's',
        b'T',
        b'a',
        b'g',
        b's',
        (comment.len() & 0xFF) as u8,
        ((comment.len() >> 8) & 0xFF) as u8,
        ((comment.len() >> 16) & 0xFF) as u8,
        ((comment.len() >> 24) & 0xFF) as u8,
    ];
    header.extend_from_slice(comment);
    header.extend_from_slice(&[0, 0, 0, 0]); // No additional comments

    packet_writer.write_packet(header, 0, PacketWriteEndInfo::NormalPacket, 0)?;

    Ok(())
}

fn write_u32(output: &mut Vec<u8>, value: u32) -> Result<(), AudioError> {
    output.write_all(&value.to_le_bytes())?;
    Ok(())
}
