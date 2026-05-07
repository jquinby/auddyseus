use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::{App, Panel};
use crate::browser::is_m3u;

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.size();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(size);

    draw_now_playing(f, app, chunks[0]);
    draw_main(f, app, chunks[1]);
    draw_help(f, app, chunks[2]);
}

fn draw_now_playing(f: &mut Frame, app: &App, area: Rect) {
    let np = app.player.now_playing.lock().unwrap();
    let status = if np.playing { "▶" } else { "⏹" };
    let vol_pct = (np.volume * 100.0).round() as u8;
    let vol_bar = volume_bar(np.volume);

    let title_text = if np.title.is_empty() { "—".to_string() } else { np.title.clone() };
    let station_text = if np.station.is_empty() {
        String::new()
    } else {
        format!("  [{}]", np.station)
    };

    let line1 = Line::from(vec![
        Span::styled(format!(" {status} "), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(title_text, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(station_text, Style::default().fg(Color::Cyan)),
    ]);
    let line2 = Line::from(vec![
        Span::styled(format!("    Vol: {vol_bar} {vol_pct}%"), Style::default().fg(Color::Yellow)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" auddyseus ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)));

    f.render_widget(Paragraph::new(vec![line1, line2]).block(block), area);
}

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_streams_panel(f, app, chunks[0]);
    draw_files_panel(f, app, chunks[1]);
}

fn draw_streams_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let active = matches!(app.active_panel, Panel::Streams);
    let border_style = if active { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) };

    let items: Vec<ListItem> = app
        .config
        .streams
        .iter()
        .map(|s| ListItem::new(format!(" {}", s.name)))
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.stream_selected));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(Span::styled(
                    " Internet Radio ",
                    Style::default().fg(if active { Color::Cyan } else { Color::Gray }),
                )),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut state);
}

fn draw_files_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let active = matches!(app.active_panel, Panel::Files);
    let border_style = if active { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) };

    let dir_display = app.browser.current_dir.to_string_lossy().to_string();

    let items: Vec<ListItem> = app
        .browser
        .entries
        .iter()
        .map(|e| {
            if e.is_dir {
                ListItem::new(format!("📁 {}", e.name)).style(Style::default().fg(Color::Yellow))
            } else if is_m3u(&e.path) {
                ListItem::new(format!("≋  {}", e.name)).style(Style::default().fg(Color::Green))
            } else {
                ListItem::new(format!("♪  {}", e.name)).style(Style::default().fg(Color::White))
            }
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.browser.selected));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(Span::styled(
                    format!(" Files: {} ", truncate(&dir_display, area.width.saturating_sub(12) as usize)),
                    Style::default().fg(if active { Color::Cyan } else { Color::Gray }),
                )),
        )
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut state);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let content = if let Some(ref msg) = app.status_msg {
        Line::from(vec![
            Span::styled(" ✓ ", Style::default().fg(Color::Green)),
            Span::styled(msg.clone(), Style::default().fg(Color::White)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Tab", Style::default().fg(Color::Yellow)),
            Span::raw(":panel  "),
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(":nav  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(":play/open  "),
            Span::styled("Space", Style::default().fg(Color::Yellow)),
            Span::raw(":pause  "),
            Span::styled("s", Style::default().fg(Color::Yellow)),
            Span::raw(":stop  "),
            Span::styled("+/-", Style::default().fg(Color::Yellow)),
            Span::raw(":vol  "),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(":quit  "),
            Span::styled("≋", Style::default().fg(Color::Green)),
            Span::raw("=m3u"),
        ])
    };
    f.render_widget(Paragraph::new(content).style(Style::default().fg(Color::DarkGray)), area);
}

fn volume_bar(vol: f32) -> String {
    let filled = (vol * 10.0).round() as usize;
    let empty = 10usize.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[s.len().saturating_sub(max)..] }
}
