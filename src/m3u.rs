use std::path::Path;
use crate::config::Stream;

/// Parse an M3U or M3U8 file into a list of Streams.
/// Handles both simple (bare URLs) and extended (#EXTM3U / #EXTINF) formats.
pub fn parse_m3u(path: &Path) -> Result<Vec<Stream>, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;

    let mut streams = Vec::new();
    let mut pending_name: Option<String> = None;

    for line in contents.lines() {
        let line = line.trim();

        if line.is_empty() || line == "#EXTM3U" {
            continue;
        }

        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            // #EXTINF:-1,Title of Station
            // duration is before the comma, title after
            if let Some((_dur, title)) = rest.split_once(',') {
                let t = title.trim().to_string();
                if !t.is_empty() {
                    pending_name = Some(t);
                }
            }
            continue;
        }

        if line.starts_with('#') {
            continue; // other comment/directive
        }

        // It's a URL or file path
        if line.starts_with("http://") || line.starts_with("https://") {
            let name = pending_name.take().unwrap_or_else(|| {
                // Derive a readable name from the URL
                line.split('/')
                    .last()
                    .unwrap_or(line)
                    .split('?')
                    .next()
                    .unwrap_or(line)
                    .to_string()
            });
            streams.push(Stream {
                name,
                url: line.to_string(),
            });
        }
        // (local file paths in M3U are ignored — we have the browser for those)
    }

    Ok(streams)
}
