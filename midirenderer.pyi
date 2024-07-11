from typing import Union, Literal

def render_wave_from(soundfont_bytes: bytes, midi_bytes: bytes) -> bytes:
    """
    Render a MIDI file to WAV format using the provided SoundFont.

    This function takes the raw bytes of a SoundFont file and a MIDI file,
    and returns the rendered audio as WAV file bytes. The rendering is performed
    using a high-performance Rust backend for optimal speed and efficiency.

    Args:
        soundfont_bytes (bytes): The raw bytes of a SoundFont (.sf2) file.
        midi_bytes (bytes): The raw bytes of a MIDI (.mid) file.

    Returns:
        bytes: The rendered audio as WAV file bytes.

    Raises:
        ValueError: If the input bytes are invalid or cannot be processed.
        RuntimeError: If there's an error during the rendering process.

    Example:
        >>> from pathlib import Path
        >>> soundfont_path = Path("path/to/soundfont.sf2")
        >>> midi_path = Path("path/to/midi_file.mid")
        >>> wav_data = render_wave_from(soundfont_path.read_bytes(), midi_path.read_bytes())
        >>> with open("output.wav", "wb") as f:
        ...     f.write(wav_data)
    """
    ...

def render_opus_from(
    soundfont_bytes: bytes,
    midi_bytes: bytes,
    stereo: bool = True,
    bitrate: Union[Literal["auto", "max"], str] = "auto"
) -> bytes:
    """
    Render a MIDI file to Opus format using the provided SoundFont.

    This function takes the raw bytes of a SoundFont file and a MIDI file,
    and returns the rendered audio as Opus file bytes. The rendering is performed
    using a high-performance Rust backend for optimal speed and efficiency.

    Args:
        soundfont_bytes (bytes): The raw bytes of a SoundFont (.sf2) file.
        midi_bytes (bytes): The raw bytes of a MIDI (.mid) file.
        stereo (bool, optional): Whether to render in stereo. Defaults to True.
        bitrate (Union[Literal["auto", "max"], str], optional): The bitrate for Opus encoding.
            Can be "auto", "max", or a string representing bits per second (e.g., "128000" for 128 kbps).
            Defaults to "auto".

    Returns:
        bytes: The rendered audio as Opus file bytes.

    Raises:
        ValueError: If the input bytes are invalid or cannot be processed.
        RuntimeError: If there's an error during the rendering process.

    Example:
        >>> from pathlib import Path
        >>> soundfont_path = Path("path/to/soundfont.sf2")
        >>> midi_path = Path("path/to/midi_file.mid")
        >>> opus_data = render_opus_from(
        ...     soundfont_path.read_bytes(),
        ...     midi_path.read_bytes(),
        ...     stereo=True,
        ...     bitrate="128000"
        ... )
        >>> with open("output.opus", "wb") as f:
        ...     f.write(opus_data)
    """
    ...

# Version of the midirenderer package
__version__: str

# Version of the Rust backend
__rust_version__: str
