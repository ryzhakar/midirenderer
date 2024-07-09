use rustysynth::{MidiFile, MidiFileSequencer, SoundFont, Synthesizer, SynthesizerSettings};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::io::{Cursor, Write};
use std::sync::Arc;

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}


#[pyfunction]
fn render_midi_with<'py>(
    py: Python<'py>,
    soundfont_bytes: &[u8],
    midi_bytes: &[u8]
) -> PyResult<Bound<'py, PyBytes>> {
    // Load the SoundFont from bytes
    let mut sf2 = Cursor::new(soundfont_bytes);
    let sound_font = Arc::new(SoundFont::new(&mut sf2)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("SoundFont error: {}", e)))?);

    // Load the MIDI file from bytes
    let mut mid = Cursor::new(midi_bytes);
    let midi_file = Arc::new(MidiFile::new(&mut mid)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("MIDI file error: {}", e)))?);

    // Create the synthesizer and sequencer
    let sample_rate = 44100;
    let settings = SynthesizerSettings::new(sample_rate);
    let synthesizer = Synthesizer::new(&sound_font, &settings)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Synthesizer error: {}", e)))?;
    let mut sequencer = MidiFileSequencer::new(synthesizer);

    // Prepare to play the MIDI file
    sequencer.play(&midi_file, false);

    // Calculate the total number of samples
    let sample_count = (sample_rate as f64 * midi_file.get_length()) as usize;
    
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
    write_u32(&mut wav_data, sample_rate as u32); // Sample rate
    write_u32(&mut wav_data, sample_rate as u32 * 4); // Byte rate
    wav_data.extend_from_slice(&4u16.to_le_bytes()); // Block align
    wav_data.extend_from_slice(&16u16.to_le_bytes()); // Bits per sample

    // Write data chunk header
    wav_data.extend_from_slice(b"data");
    write_u32(&mut wav_data, (sample_count * 4) as u32); // Chunk size

    // Convert f32 samples to i16 and write to WAV data
    for (l, r) in left.iter().zip(right.iter()) {
        let left_sample = (l.max(-1.0).min(1.0) * 32767.0) as i16;
        let right_sample = (r.max(-1.0).min(1.0) * 32767.0) as i16;
        wav_data.write_all(&left_sample.to_le_bytes()).unwrap();
        wav_data.write_all(&right_sample.to_le_bytes()).unwrap();
    }

    // Return the WAV data as Python bytes
    Ok(PyBytes::new_bound(py, &wav_data))
}

#[pymodule]
fn midirenderer(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render_midi_with, m)?)?;
    Ok(())
}
