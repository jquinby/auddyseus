use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Stream {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default = "default_music_dir")]
    pub music_dir: String,
    #[serde(default)]
    pub streams: Vec<Stream>,
    #[serde(default = "default_notifications")]
    pub notifications: bool,
}

fn default_music_dir() -> String {
    dirs::audio_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .to_string_lossy()
        .to_string()
}

fn default_notifications() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            music_dir: default_music_dir(),
            streams: vec![
                Stream { name: "SomaFM: Groove Salad".into(), url: "http://ice1.somafm.com/groovesalad-128-mp3".into() },
                Stream { name: "SomaFM: Drone Zone".into(), url: "http://ice1.somafm.com/dronezone-128-mp3".into() },
                Stream { name: "SomaFM: Secret Agent".into(), url: "http://ice1.somafm.com/secretagent-128-mp3".into() },
                Stream { name: "SomaFM: Space Station".into(), url: "http://ice1.somafm.com/spacestation-128-mp3".into() },
                Stream { name: "SomaFM: Lush".into(), url: "http://ice1.somafm.com/lush-128-mp3".into() },
                Stream { name: "Radio Paradise (Main)".into(), url: "http://stream.radioparadise.com/mp3-128".into() },
            ],
            notifications: true,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(contents) = std::fs::read_to_string(&path) {
            toml::from_str(&contents).unwrap_or_default()
        } else {
            let default = Config::default();
            // Write default config so user can edit it
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(s) = toml::to_string_pretty(&default) {
                let _ = std::fs::write(&path, s);
            }
            default
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("auddyseus")
        .join("config.toml")
}
