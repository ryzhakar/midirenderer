use ogg::{writing::PacketWriteEndInfo, PacketWriter};
use opus::{Application, Bitrate, Channels, Encoder};
use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use std::io::{Cursor, Write};
use std::sync::Arc;

const SAMPLE_RATE: i32 = 48000;
const FRAME_SIZE: usize = 960; // 20ms at 48kHz
const MAX_PACKET_SIZE: usize = 1275; // Maximum size of an Opus packet

pub enum OpusBitrate {
    Auto,
    Max,
    Bits(i32),
}

pub fn render_midi_to_wav(
    soundfont_bytes: &[u8],
    midi_bytes: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Load the SoundFont from bytes
    let mut sf2 = Cursor::new(soundfont_bytes);
    let sound_font = Arc::new(SoundFont::new(&mut sf2)?);

    // Load the MIDI file from bytes
    let mut mid = Cursor::new(midi_bytes);
    let midi_file = Arc::new(MidiFile::new(&mut mid)?);

    // Create the synthesizer and sequencer
    let settings = SynthesizerSettings::new(SAMPLE_RATE);
    let synthesizer = Synthesizer::new(&sound_font, &settings)?;
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
    write_u32(&mut wav_data, 36 + (sample_count * 4) as u32); // File size - 8
    wav_data.extend_from_slice(b"WAVE");

    // Write format chunk
    wav_data.extend_from_slice(b"fmt ");
    write_u32(&mut wav_data, 16); // Chunk size
    wav_data.extend_from_slice(&1u16.to_le_bytes()); // Audio format (PCM)
    wav_data.extend_from_slice(&2u16.to_le_bytes()); // Number of channels
    write_u32(&mut wav_data, SAMPLE_RATE as u32); // Sample rate
    write_u32(&mut wav_data, (SAMPLE_RATE as u32) * 4); // Byte rate
    wav_data.extend_from_slice(&4u16.to_le_bytes()); // Block align
    wav_data.extend_from_slice(&16u16.to_le_bytes()); // Bits per sample

    // Write data chunk header
    wav_data.extend_from_slice(b"data");
    write_u32(&mut wav_data, (sample_count * 4) as u32); // Chunk size

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
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Skip WAV header (44 bytes) and convert to f32
    let pcm_data = &wav_data[44..];
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

    // Encode and write
    let channel_count = if stereo { 2 } else { 1 };
    let mut granule_position = 0;
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
    packet_writer.write_packet(Vec::new(), 0, PacketWriteEndInfo::EndStream, 0)?;
    drop(packet_writer);

    Ok(ogg_output)
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
