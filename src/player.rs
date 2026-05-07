use rodio::{OutputStream, Sink, Source};
use rodio::buffer::SamplesBuffer;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug, Clone, Default)]
pub struct NowPlaying {
    pub title: String,
    pub station: String,
    pub is_stream: bool,
    pub playing: bool,
    pub volume: f32,
}

pub enum PlayerCommand {
    PlayStream { name: String, url: String },
    PlayLocal(std::path::PathBuf),
    Stop,
    Pause,
    Resume,
    VolumeUp,
    VolumeDown,
    Quit,
}

pub struct Player {
    pub cmd_tx: std::sync::mpsc::Sender<PlayerCommand>,
    pub now_playing: Arc<Mutex<NowPlaying>>,
}

impl Player {
    pub fn new(notifications: bool) -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<PlayerCommand>();
        let now_playing = Arc::new(Mutex::new(NowPlaying {
            volume: 1.0,
            ..Default::default()
        }));
        let np_clone = Arc::clone(&now_playing);
        thread::spawn(move || player_loop(cmd_rx, np_clone, notifications));
        Player { cmd_tx, now_playing }
    }

    pub fn send(&self, cmd: PlayerCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

// ── Streaming pipe ─────────────────────────────────────────────────────────

struct StreamPipeReader {
    rx: Mutex<std::sync::mpsc::Receiver<Vec<u8>>>,
    leftovers: Mutex<std::collections::VecDeque<u8>>,
}
impl Read for StreamPipeReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let mut buf = self.leftovers.lock().unwrap();
        while buf.is_empty() {
            match self.rx.lock().unwrap().recv() {
                Ok(chunk) => buf.extend(chunk),
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(buf.len());
        for (dst, src) in out[..n].iter_mut().zip(buf.drain(..n)) { *dst = src; }
        Ok(n)
    }
}
impl Seek for StreamPipeReader {
    fn seek(&mut self, _: SeekFrom) -> std::io::Result<u64> { Ok(0) }
}
impl MediaSource for StreamPipeReader {
    fn is_seekable(&self) -> bool { false }
    fn byte_len(&self) -> Option<u64> { None }
}
unsafe impl Send for StreamPipeReader {}
unsafe impl Sync for StreamPipeReader {}

fn make_pipe() -> (std::sync::mpsc::SyncSender<Vec<u8>>, StreamPipeReader) {
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(64);
    (tx, StreamPipeReader {
        rx: Mutex::new(rx),
        leftovers: Mutex::new(std::collections::VecDeque::new()),
    })
}

// ── Symphonia source (MP3, OGG, FLAC, MPEG-4 AAC streams) ─────────────────

struct SymphoniaStreamSource {
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: u16,
    buf: std::collections::VecDeque<f32>,
}

impl SymphoniaStreamSource {
    fn new(mss: MediaSourceStream, ext_hint: Option<&str>) -> Result<Self, String> {
        let mut hint = Hint::new();
        if let Some(e) = ext_hint { hint.with_extension(e); }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss,
                &FormatOptions { enable_gapless: true, ..Default::default() },
                &MetadataOptions::default())
            .map_err(|e| format!("Probe failed: {e}"))?;
        let fmt = probed.format;
        let track = fmt.tracks().iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
            .ok_or("No playable track")?;
        let track_id = track.id;
        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(2);
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| format!("Codec: {e}"))?;
        Ok(Self { format: fmt, decoder, track_id, sample_rate, channels, buf: Default::default() })
    }

    // Decode packets until buf has at least min_samples f32 samples.
    // Keeping a 1-second cushion ahead of playback eliminates ALSA underruns.
    fn fill_to(&mut self, min_samples: usize) -> bool {
        while self.buf.len() < min_samples {
            let pkt = match self.format.next_packet() {
                Ok(p) => p,
                Err(_) => return !self.buf.is_empty(),
            };
            if pkt.track_id() != self.track_id { continue; }
            match self.decoder.decode(&pkt) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let mut sb = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                    sb.copy_interleaved_ref(decoded);
                    self.buf.extend(sb.samples().iter().copied());
                }
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(_) => return !self.buf.is_empty(),
            }
        }
        true
    }
}
impl Iterator for SymphoniaStreamSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        // Refill when buffer drops below 2048 samples; target ~1s cushion
        if self.buf.len() < 2048 && !self.fill_to(44100) && self.buf.is_empty() {
            return None;
        }
        self.buf.pop_front()
    }
}
impl Source for SymphoniaStreamSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { self.channels }
    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn total_duration(&self) -> Option<Duration> { None }
}

// ── ffmpeg PCM source (MPEG-2 AAC / HE-AAC and anything else exotic) ───────
//
// For streams where symphonia probe fails — primarily MPEG-2 ADTS AAC
// (SomaFM's -aac URLs, sync word 0xfff9) — we spawn:
//   ffmpeg -i <url> -vn -f s16le -ar 44100 -ac 2 pipe:1
// and feed the raw PCM stdout into rodio as a SamplesBuffer stream.

struct FfmpegSource {
    child: std::process::Child,
    stdout: std::io::BufReader<std::process::ChildStdout>,
    sample_rate: u32,
    channels: u16,
    buf: Vec<i16>,
    pos: usize,
    done: bool,
}

impl FfmpegSource {
    fn new(url: &str) -> Result<Self, String> {
        let mut child = Command::new("ffmpeg")
            .args([
                "-loglevel", "quiet",
                "-i", url,
                "-vn",
                "-f", "s16le",
                "-ar", "44100",
                "-ac", "2",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| format!("ffmpeg spawn failed: {e}"))?;
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Ok(Self { child, stdout, sample_rate: 44100, channels: 2, buf: vec![0i16; 4096], pos: 0, done: false })
    }

    fn fill(&mut self) -> bool {
        if self.done { return false; }
        // Read a chunk of raw s16le bytes
        let byte_buf: &mut [u8] = unsafe {
            std::slice::from_raw_parts_mut(
                self.buf.as_mut_ptr() as *mut u8,
                self.buf.len() * 2,
            )
        };
        match self.stdout.read(byte_buf) {
            Ok(0) => { self.done = true; false }
            Ok(n) => { self.pos = 0; self.buf.truncate(n / 2); true }
            Err(_) => { self.done = true; false }
        }
    }
}

impl Drop for FfmpegSource {
    fn drop(&mut self) { let _ = self.child.kill(); }
}

impl Iterator for FfmpegSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.pos >= self.buf.len() {
            // Refill — but first restore buf capacity
            self.buf.resize(4096, 0);
            if !self.fill() { return None; }
        }
        let sample = self.buf[self.pos] as f32 / 32768.0;
        self.pos += 1;
        Some(sample)
    }
}

impl Source for FfmpegSource {
    fn current_frame_len(&self) -> Option<usize> { None }
    fn channels(&self) -> u16 { self.channels }
    fn sample_rate(&self) -> u32 { self.sample_rate }
    fn total_duration(&self) -> Option<Duration> { None }
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn url_extension(url: &str) -> Option<String> {
    url.split('?').next()
        .and_then(|u| u.split('/').last())
        .and_then(|seg| {
            let ext = seg.rsplit('.').next()?;
            if ext == seg { None } else { Some(ext.to_lowercase()) }
        })
        .filter(|e| e.len() <= 5)
}

/// Returns true if the URL looks like it wants AAC
/// (either by extension, or the SomaFM "-aac" suffix convention)
fn looks_like_aac(url: &str) -> bool {
    let lower = url.to_lowercase();
    url_extension(url).map(|e| e == "aac").unwrap_or(false)
        || lower.ends_with("-aac")
        || lower.contains("-aac?")
        || lower.contains("/aac-")
        || lower.contains("audio/aacp")
        || lower.contains("audio/aac")
}

// ── Stop helper ────────────────────────────────────────────────────────────

fn stop_all(
    sink: &mut Option<Arc<Mutex<Sink>>>,
    fetch_guard: &mut Option<std::sync::mpsc::SyncSender<Vec<u8>>>,
) {
    *fetch_guard = None;
    if let Some(s) = sink.take() { s.lock().unwrap().stop(); }
}

// ── Player loop ────────────────────────────────────────────────────────────

fn player_loop(
    rx: std::sync::mpsc::Receiver<PlayerCommand>,
    now_playing: Arc<Mutex<NowPlaying>>,
    notifications: bool,
) {
    let (_stream, stream_handle) =
        OutputStream::try_default().expect("No audio output device found");
    let mut sink: Option<Arc<Mutex<Sink>>> = None;
    let mut volume: f32 = 1.0;
    let mut fetch_guard: Option<std::sync::mpsc::SyncSender<Vec<u8>>> = None;

    loop {
        match rx.recv() {
            Ok(PlayerCommand::PlayStream { name, url }) => {
                stop_all(&mut sink, &mut fetch_guard);

                {
                    let mut np = now_playing.lock().unwrap();
                    np.playing = true;
                    np.is_stream = true;
                    np.station = name.clone();
                    np.title = "Connecting…".into();
                }

                let new_sink = Arc::new(Mutex::new(
                    Sink::try_new(&stream_handle).expect("Could not create sink"),
                ));
                new_sink.lock().unwrap().set_volume(volume);
                sink = Some(Arc::clone(&new_sink));
                let np_err = Arc::clone(&now_playing);

                if looks_like_aac(&url) {
                    // Path A: ffmpeg handles MPEG-2/HE-AAC streams directly
                    let url2 = url.clone();
                    let name2 = name.clone();
                    let np2 = Arc::clone(&now_playing);
                    thread::spawn(move || {
                        // ICY metadata fetch in parallel for notifications
                        // (ffmpeg doesn't expose ICY, so we do a lightweight side-fetch)
                        let np3 = Arc::clone(&np2);
                        let url3 = url2.clone();
                        let name3 = name2.clone();
                        thread::spawn(move || {
                            icy_metadata_thread(url3, name3, np3, notifications);
                        });

                        match FfmpegSource::new(&url2) {
                            Ok(source) => {
                                new_sink.lock().unwrap().append(source);
                            }
                            Err(e) => {
                                np2.lock().unwrap().title = format!("ffmpeg error: {e}");
                            }
                        }
                    });
                } else {
                    // Path B: symphonia via pipe (MP3, OGG, MPEG-4 AAC, etc.)
                    let (audio_tx, audio_rx) = make_pipe();
                    fetch_guard = Some(audio_tx.clone());
                    let ext_hint = url_extension(&url);
                    let np2 = Arc::clone(&now_playing);
                    let np3 = Arc::clone(&now_playing);

                    thread::spawn(move || {
                        fetch_stream(url, name, audio_tx, np2, notifications);
                    });

                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(1500));
                        let mss = MediaSourceStream::new(Box::new(audio_rx), Default::default());
                        match SymphoniaStreamSource::new(mss, ext_hint.as_deref()) {
                            Ok(source) => { new_sink.lock().unwrap().append(source); }
                            Err(e) => { np3.lock().unwrap().title = format!("Decode error: {e}"); }
                        }
                    });
                }
            }

            Ok(PlayerCommand::PlayLocal(path)) => {
                stop_all(&mut sink, &mut fetch_guard);
                let filename = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                match File::open(&path) {
                    Ok(f) => match rodio::Decoder::new(BufReader::new(f)) {
                        Ok(source) => {
                            let s = Sink::try_new(&stream_handle).unwrap();
                            s.set_volume(volume);
                            s.append(source);
                            {
                                let mut np = now_playing.lock().unwrap();
                                np.playing = true; np.is_stream = false;
                                np.station = "Local".into(); np.title = filename.clone();
                            }
                            if notifications { send_notification("auddyseus", &filename, ""); }
                            sink = Some(Arc::new(Mutex::new(s)));
                        }
                        Err(e) => { now_playing.lock().unwrap().title = format!("Decode error: {e}"); }
                    },
                    Err(e) => { now_playing.lock().unwrap().title = format!("Open error: {e}"); }
                }
            }

            Ok(PlayerCommand::Stop) => {
                stop_all(&mut sink, &mut fetch_guard);
                let mut np = now_playing.lock().unwrap();
                np.playing = false; np.title = String::new();
            }
            Ok(PlayerCommand::Pause) => {
                if let Some(ref s) = sink { s.lock().unwrap().pause(); }
                now_playing.lock().unwrap().playing = false;
            }
            Ok(PlayerCommand::Resume) => {
                if let Some(ref s) = sink { s.lock().unwrap().play(); }
                now_playing.lock().unwrap().playing = true;
            }
            Ok(PlayerCommand::VolumeUp) => {
                volume = (volume + 0.05).min(1.0);
                if let Some(ref s) = sink { s.lock().unwrap().set_volume(volume); }
                now_playing.lock().unwrap().volume = volume;
            }
            Ok(PlayerCommand::VolumeDown) => {
                volume = (volume - 0.05).max(0.0);
                if let Some(ref s) = sink { s.lock().unwrap().set_volume(volume); }
                now_playing.lock().unwrap().volume = volume;
            }
            Ok(PlayerCommand::Quit) | Err(_) => {
                stop_all(&mut sink, &mut fetch_guard);
                break;
            }
        }
    }
}

// ── ICY metadata side-fetch for ffmpeg path ────────────────────────────────
// ffmpeg doesn't forward ICY metadata to us, so for AAC streams we open a
// second, low-bandwidth connection just to read metadata blocks.

fn icy_metadata_thread(
    url: String,
    station_name: String,
    now_playing: Arc<Mutex<NowPlaying>>,
    notifications: bool,
) {
    let client = match reqwest::blocking::Client::builder()
        .timeout(None).connect_timeout(Duration::from_secs(15)).build()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let response = match client.get(&url)
        .header("Icy-MetaData", "1")
        .header("User-Agent", "auddyseus-meta/0.1")
        .send()
    {
        Ok(r) => r,
        Err(_) => return,
    };
    let metaint: usize = response.headers()
        .get("icy-metaint")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if metaint == 0 { return; }

    let mut reader = response;
    let mut skip_buf = vec![0u8; 4096];
    let mut bytes_since_meta = 0usize;
    let mut last_title = String::new();

    loop {
        let to_skip = (metaint - bytes_since_meta).min(skip_buf.len());
        match reader.read(&mut skip_buf[..to_skip]) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                bytes_since_meta += n;
                if bytes_since_meta >= metaint {
                    bytes_since_meta = 0;
                    let mut len_buf = [0u8; 1];
                    if reader.read_exact(&mut len_buf).is_err() { return; }
                    let meta_len = (len_buf[0] as usize) * 16;
                    if meta_len > 0 {
                        let mut meta_buf = vec![0u8; meta_len];
                        if reader.read_exact(&mut meta_buf).is_err() { return; }
                        if let Ok(s) = std::str::from_utf8(&meta_buf) {
                            let title = parse_icy_title(s);
                            if !title.is_empty() && title != last_title {
                                last_title = title.clone();
                                {
                                    let mut np = now_playing.lock().unwrap();
                                    np.title = title.clone();
                                    np.station = station_name.clone();
                                    np.playing = true;
                                }
                                if notifications { send_notification(&station_name, &title, ""); }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── fetch_stream (symphonia path only) ────────────────────────────────────

fn fetch_stream(
    url: String,
    station_name: String,
    audio_tx: std::sync::mpsc::SyncSender<Vec<u8>>,
    now_playing: Arc<Mutex<NowPlaying>>,
    notifications: bool,
) {
    let client = match reqwest::blocking::Client::builder()
        .timeout(None).connect_timeout(Duration::from_secs(15)).build()
    {
        Ok(c) => c,
        Err(e) => { now_playing.lock().unwrap().title = format!("HTTP error: {e}"); return; }
    };
    let response = match client.get(&url)
        .header("Icy-MetaData", "1")
        .header("User-Agent", "auddyseus/0.1")
        .send()
    {
        Ok(r) => r,
        Err(e) => { now_playing.lock().unwrap().title = format!("Connection failed: {e}"); return; }
    };
    let metaint: usize = response.headers()
        .get("icy-metaint")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let mut reader = response;
    let mut read_buf = vec![0u8; 8192];
    let mut bytes_since_meta = 0usize;
    let mut last_title = String::new();

    loop {
        let to_read = if metaint > 0 { (metaint - bytes_since_meta).min(read_buf.len()) } else { read_buf.len() };
        match reader.read(&mut read_buf[..to_read]) {
            Ok(0) => { now_playing.lock().unwrap().title = "Stream ended".into(); return; }
            Ok(n) => {
                bytes_since_meta += n;
                if audio_tx.send(read_buf[..n].to_vec()).is_err() { return; }
                if metaint > 0 && bytes_since_meta >= metaint {
                    bytes_since_meta = 0;
                    let mut len_buf = [0u8; 1];
                    if reader.read_exact(&mut len_buf).is_err() { return; }
                    let meta_len = (len_buf[0] as usize) * 16;
                    if meta_len > 0 {
                        let mut meta_buf = vec![0u8; meta_len];
                        if reader.read_exact(&mut meta_buf).is_err() { return; }
                        if let Ok(s) = std::str::from_utf8(&meta_buf) {
                            let title = parse_icy_title(s);
                            if !title.is_empty() && title != last_title {
                                last_title = title.clone();
                                { let mut np = now_playing.lock().unwrap(); np.title = title.clone(); np.station = station_name.clone(); np.playing = true; }
                                if notifications { send_notification(&station_name, &title, ""); }
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(5)),
            Err(e) => { now_playing.lock().unwrap().title = format!("Stream error: {e}"); return; }
        }
    }
}

fn parse_icy_title(meta: &str) -> String {
    for part in meta.split(';') {
        let part = part.trim().trim_end_matches('\0');
        if let Some(rest) = part.strip_prefix("StreamTitle=") {
            return rest.trim_matches('\'').to_string();
        }
    }
    String::new()
}

pub fn send_notification(summary: &str, body: &str, _icon: &str) {
    let _ = notify_rust::Notification::new()
        .summary(summary).body(body)
        .timeout(notify_rust::Timeout::Milliseconds(4000))
        .show();
}
