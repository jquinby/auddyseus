# auddyseus

A standalone terminal music player for Linux. Plays internet radio streams (SomaFM, Icecast, SHOUTcast, etc.) and local audio files, with D-Bus desktop notifications on track change.

![auddyseus screenshot](https://github.com/jquinby/auddyseus/blob/main/screenshot.png)

## Features

- **Internet radio** — Icecast/SHOUTcast streams with ICY metadata parsing (extracts `StreamTitle` for the now-playing bar and notifications)
- **AAC streams** — HE-AAC and MPEG-2 AAC (e.g. SomaFM `-aac` URLs) via ffmpeg; MP3/OGG/FLAC streams via symphonia
- **Local files** — filesystem browser; plays MP3, FLAC, OGG, Opus, M4A, WAV and more
- **M3U playlists** — open any `.m3u`/`.m3u8` file from the browser to load its streams into the radio panel
- **D-Bus notifications** — fires a desktop notification on every track change via `libnotify`
- **Clean TUI** — two-panel layout, arrow-key navigation, no cryptic bindings

## Keys

| Key | Action |
|-----|--------|
| `Tab` | Switch between Internet Radio and Files panels |
| `↑` / `↓` | Navigate |
| `PgUp` / `PgDn` | Page through long lists |
| `Enter` | Play stream / enter directory / play file / load M3U |
| `Space` | Pause / Resume |
| `s` | Stop |
| `+` / `-` | Volume up / down |
| `q` | Quit |

## Installation

### Dependencies

```bash
sudo apt install libasound2-dev libssl-dev pkg-config ffmpeg
```

`ffmpeg` is required for AAC stream playback. Everything else is handled by Rust/Cargo.

### Build and install

```bash
git clone https://github.com/YOUR_USERNAME/auddyseus
cd auddyseus
bash install.sh
```

Or manually:

```bash
cargo build --release
cp target/release/auddyseus ~/.local/bin/
```

Ensure `~/.local/bin` is in your `$PATH`.

## Configuration

On first run, a config file is created at `~/.config/auddyseus/config.toml`:

```toml
music_dir = "/home/you/Music"
notifications = true

[[streams]]
name = "SomaFM: Groove Salad"
url  = "http://ice1.somafm.com/groovesalad-128-mp3"

[[streams]]
name = "SomaFM: Groove Salad (AAC)"
url  = "https://ice2.somafm.com/groovesalad-128-aac"

[[streams]]
name = "My Station"
url  = "http://example.com/stream.mp3"
```

Any Icecast/SHOUTcast HTTP(S) stream URL works. URLs ending in `-aac`, `.aac`, or containing `/aac-` are automatically routed through ffmpeg for full HE-AAC support.

You can also navigate to any `.m3u` file in the file browser and press `Enter` to load its streams into the Internet Radio panel for the current session.

## Architecture

```
src/
  main.rs      — terminal setup, event loop
  app.rs       — application state, key handling
  ui.rs        — ratatui layout and rendering
  player.rs    — audio engine (symphonia + ffmpeg paths, ICY metadata, D-Bus notifications)
  browser.rs   — filesystem navigation
  config.rs    — TOML config loading/saving
  m3u.rs       — M3U/M3U8 parser
```

Streams are played via two paths:
- **MP3/OGG/FLAC**: HTTP fetch thread → `mpsc` pipe → symphonia decoder → rodio sink
- **AAC/HE-AAC**: `ffmpeg -i <url> -f s16le pipe:1` → rodio sink, with a parallel ICY metadata connection for notifications

## License

MIT
