use std::path::{Path, PathBuf};

pub struct FileBrowser {
    pub current_dir: PathBuf,
    pub entries: Vec<BrowserEntry>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct BrowserEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

impl FileBrowser {
    pub fn new(root: &str) -> Self {
        let path = PathBuf::from(root);
        let mut browser = FileBrowser {
            current_dir: path.clone(),
            entries: vec![],
            selected: 0,
        };
        browser.load_dir(&path);
        browser
    }

    pub fn load_dir(&mut self, path: &Path) {
        self.entries.clear();
        self.selected = 0;

        // Add ".." entry if not at filesystem root
        if path.parent().is_some() {
            self.entries.push(BrowserEntry {
                name: "..".into(),
                path: path.parent().unwrap().to_path_buf(),
                is_dir: true,
            });
        }

        let mut dirs: Vec<BrowserEntry> = vec![];
        let mut files: Vec<BrowserEntry> = vec![];

        if let Ok(read) = std::fs::read_dir(path) {
            for entry in read.flatten() {
                let p = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue; // skip hidden
                }
                let is_dir = p.is_dir();
                if is_dir {
                    dirs.push(BrowserEntry { name, path: p, is_dir: true });
                } else if is_audio(&p) || is_m3u(&p) {
                    files.push(BrowserEntry { name, path: p, is_dir: false });
                }
            }
        }

        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        self.entries.extend(dirs);
        self.entries.extend(files);
        self.current_dir = path.to_path_buf();
    }

    pub fn enter(&mut self) -> Option<PathBuf> {
        if let Some(entry) = self.entries.get(self.selected) {
            if entry.is_dir {
                let path = entry.path.clone();
                self.load_dir(&path);
                None
            } else {
                Some(entry.path.clone())
            }
        } else {
            None
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.selected = self.selected.saturating_sub(page_size);
    }

    pub fn page_down(&mut self, page_size: usize) {
        self.selected = (self.selected + page_size).min(self.entries.len().saturating_sub(1));
    }

    pub fn go_top(&mut self) {
        self.selected = 0;
    }

    pub fn go_bottom(&mut self) {
        if !self.entries.is_empty() {
            self.selected = self.entries.len() - 1;
        }
    }
}

pub fn is_audio(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .as_deref(),
        Some("mp3" | "flac" | "ogg" | "opus" | "m4a" | "aac" | "wav" | "aiff" | "wv" | "ape")
    )
}

pub fn is_m3u(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .as_deref(),
        Some("m3u" | "m3u8")
    )
}
