use crossterm::event::{Event, KeyCode, KeyEvent};

use crate::browser::{is_m3u, FileBrowser};
use crate::config::{Config, Stream};
use crate::m3u::parse_m3u;
use crate::player::{Player, PlayerCommand};

pub enum Panel {
    Streams,
    Files,
}

pub struct App {
    pub config: Config,
    pub player: Player,
    pub browser: FileBrowser,
    pub active_panel: Panel,
    pub stream_selected: usize,
    pub should_quit: bool,
    pub status_msg: Option<String>,
}

impl App {
    pub fn new(config: Config) -> Self {
        let notifications = config.notifications;
        let browser = FileBrowser::new(&config.music_dir);
        let player = Player::new(notifications);

        App {
            browser,
            player,
            active_panel: Panel::Streams,
            stream_selected: 0,
            should_quit: false,
            config,
            status_msg: None,
        }
    }

    pub fn handle_event(&mut self, event: Event) {
        if let Event::Key(KeyEvent { code, .. }) = event {
            match code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.player.send(PlayerCommand::Quit);
                    self.should_quit = true;
                }

                KeyCode::Tab => {
                    self.active_panel = match self.active_panel {
                        Panel::Streams => Panel::Files,
                        Panel::Files => Panel::Streams,
                    };
                }

                KeyCode::Up => match self.active_panel {
                    Panel::Streams => {
                        if self.stream_selected > 0 {
                            self.stream_selected -= 1;
                        }
                    }
                    Panel::Files => self.browser.move_up(),
                },

                KeyCode::Down => match self.active_panel {
                    Panel::Streams => {
                        if self.stream_selected + 1 < self.config.streams.len() {
                            self.stream_selected += 1;
                        }
                    }
                    Panel::Files => self.browser.move_down(),
                },

                KeyCode::PageUp => match self.active_panel {
                    Panel::Streams => self.stream_selected = 0,
                    Panel::Files => self.browser.page_up(10),
                },

                KeyCode::PageDown => match self.active_panel {
                    Panel::Streams => {
                        self.stream_selected =
                            self.config.streams.len().saturating_sub(1);
                    }
                    Panel::Files => self.browser.page_down(10),
                },

                KeyCode::Home => match self.active_panel {
                    Panel::Streams => self.stream_selected = 0,
                    Panel::Files => self.browser.go_top(),
                },

                KeyCode::End => match self.active_panel {
                    Panel::Streams => {
                        self.stream_selected =
                            self.config.streams.len().saturating_sub(1);
                    }
                    Panel::Files => self.browser.go_bottom(),
                },

                KeyCode::Enter => match self.active_panel {
                    Panel::Streams => {
                        if let Some(stream) = self.config.streams.get(self.stream_selected) {
                            self.player.send(PlayerCommand::PlayStream {
                                name: stream.name.clone(),
                                url: stream.url.clone(),
                            });
                        }
                    }
                    Panel::Files => {
                        if let Some(entry) = self.browser.entries.get(self.browser.selected) {
                            let path = entry.path.clone();
                            if entry.is_dir {
                                self.browser.enter();
                            } else if is_m3u(&path) {
                                // Load M3U into streams panel
                                match parse_m3u(&path) {
                                    Ok(streams) if !streams.is_empty() => {
                                        let count = streams.len();
                                        self.config.streams = streams;
                                        self.stream_selected = 0;
                                        self.active_panel = Panel::Streams;
                                        self.status_msg = Some(format!(
                                            "Loaded {} streams from {}",
                                            count,
                                            path.file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                        ));
                                    }
                                    Ok(_) => {
                                        self.status_msg =
                                            Some("M3U file contained no HTTP streams".into());
                                    }
                                    Err(e) => {
                                        self.status_msg = Some(e);
                                    }
                                }
                            } else {
                                // Regular audio file
                                if let Some(file_path) = self.browser.enter() {
                                    self.player.send(PlayerCommand::PlayLocal(file_path));
                                }
                            }
                        }
                    }
                },

                KeyCode::Char(' ') => {
                    let playing = self.player.now_playing.lock().unwrap().playing;
                    if playing {
                        self.player.send(PlayerCommand::Pause);
                    } else {
                        self.player.send(PlayerCommand::Resume);
                    }
                }

                KeyCode::Char('s') | KeyCode::Char('S') => {
                    self.player.send(PlayerCommand::Stop);
                }

                KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.player.send(PlayerCommand::VolumeUp);
                }

                KeyCode::Char('-') => {
                    self.player.send(PlayerCommand::VolumeDown);
                }

                _ => {
                    // Clear any status message on any other keypress
                    self.status_msg = None;
                }
            }
        }
    }
}
