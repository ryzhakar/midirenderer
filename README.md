# MidiRenderer

MidiRenderer is a high-performance Python library for rendering MIDI files to WAV and Opus formats using SoundFonts. Built with Rust for speed and efficiency.

## Features

- Render MIDI to WAV and Opus
- Uses SoundFont (.sf2) files
- High-performance Rust backend
- Cross-platform support (Windows, macOS, Linux, including ARM64)

## Installation

```bash
pip install midirenderer
```

## Quick Start

```python
import midirenderer
from pathlib import Path

# Render MIDI to WAV
wav_data = midirenderer.render_wave_from(
    Path('soundfont.sf2').read_bytes(),
    Path('music.mid').read_bytes()
)

with open('output.wav', 'wb') as f:
    f.write(wav_data)

# Render MIDI to Opus
opus_data = midirenderer.render_opus_from(
    Path('soundfont.sf2').read_bytes(),
    Path('music.mid').read_bytes(),
    stereo=True,
    bitrate="128000"  # 128 kbps
)

with open('output.opus', 'wb') as f:
    f.write(opus_data)
```

## API

- `render_wave_from(soundfont_bytes: bytes, midi_bytes: bytes) -> bytes`
- `render_opus_from(soundfont_bytes: bytes, midi_bytes: bytes, stereo: bool = True, bitrate: str = "auto") -> bytes`

## Requirements

- Python 3.8+
- libopus (usually pre-installed on most systems)

## License

MIT License
