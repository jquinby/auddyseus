mod app;
mod m3u;
mod browser;
mod config;
mod player;
mod ui;

use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;
use config::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load();

    // Redirect stderr to /dev/null before entering TUI.
    // ALSA and PulseAudio write diagnostic noise (underrun warnings etc.)
    // directly to stderr which bleeds through the alternate screen buffer.
    // Done after Config::load() so any startup errors are still visible.
    unsafe {
        let path = b"/dev/null\0";
        let devnull = libc::open(path.as_ptr() as *const libc::c_char, libc::O_WRONLY);
        if devnull >= 0 {
            libc::dup2(devnull, 2);
            libc::close(devnull);
        }
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config);

    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::from_millis(0));

        if event::poll(timeout)? {
            let ev = event::read()?;
            app.handle_event(ev);
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
