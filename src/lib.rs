use pyo3::prelude::*;
use pyo3::types::PyBytes;

mod audio_utils;
use audio_utils::{render_midi_to_wav, wav_to_opus_ogg, OpusBitrate};

#[pyfunction]
fn render_wave_from<'py>(
    py: Python<'py>,
    soundfont_bytes: &[u8],
    midi_bytes: &[u8],
) -> PyResult<Bound<'py, PyBytes>> {
    let wav_data = render_midi_to_wav(soundfont_bytes, midi_bytes)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
    Ok(PyBytes::new_bound(py, &wav_data))
}

#[pyfunction]
#[pyo3(signature = (soundfont_bytes, midi_bytes, stereo=true, bitrate="auto"))]
fn render_opus_from<'py>(
    py: Python<'py>,
    soundfont_bytes: &[u8],
    midi_bytes: &[u8],
    stereo: bool,
    bitrate: &str,
) -> PyResult<Bound<'py, PyBytes>> {
    let wav_data = render_midi_to_wav(soundfont_bytes, midi_bytes)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

    let opus_bitrate = match bitrate {
        "auto" => OpusBitrate::Auto,
        "max" => OpusBitrate::Max,
        _ => bitrate.parse::<i32>().map(OpusBitrate::Bits).map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>("Invalid bitrate value")
        })?,
    };

    let opus_ogg_data = wav_to_opus_ogg(&wav_data, stereo, opus_bitrate)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

    Ok(PyBytes::new_bound(py, &opus_ogg_data))
}

#[pymodule]
fn midirenderer(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render_wave_from, m)?)?;
    m.add_function(wrap_pyfunction!(render_opus_from, m)?)?;
    Ok(())
}
