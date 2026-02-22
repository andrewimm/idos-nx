use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, InputMode};

fn ansi_to_color(code: u8) -> Color {
    match code {
        30 => Color::Black,
        31 => Color::Red,
        32 => Color::Green,
        33 => Color::Yellow,
        34 => Color::Blue,
        35 => Color::Magenta,
        36 => Color::Cyan,
        37 => Color::White,
        90 => Color::DarkGray,
        91 => Color::LightRed,
        92 => Color::LightGreen,
        93 => Color::LightYellow,
        94 => Color::LightBlue,
        95 => Color::LightMagenta,
        96 => Color::LightCyan,
        97 => Color::White,
        _ => Color::White,
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let show_input = app.mode != InputMode::Normal;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if show_input {
            vec![
                Constraint::Length(3), // header
                Constraint::Min(1),   // log area
                Constraint::Length(3), // input
            ]
        } else {
            vec![
                Constraint::Length(3), // header
                Constraint::Min(1),   // log area
            ]
        })
        .split(f.area());

    // --- Header ---
    let filter_text = match &app.filter {
        Some(f) => format!("Filter: {f}"),
        None => "Filter: (none)".into(),
    };

    let search_matches = app.search_match_indices();
    let search_text = match &app.search {
        Some(s) => {
            let total = search_matches.len();
            let cur = if total > 0 {
                app.search_match_index + 1
            } else {
                0
            };
            format!("Search: /{s}  Matches: {cur}/{total}")
        }
        None => "Search: (none)".into(),
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(" [", Style::default().fg(Color::DarkGray)),
        Span::styled(&filter_text, Style::default().fg(Color::Cyan)),
        Span::styled("]  [", Style::default().fg(Color::DarkGray)),
        Span::styled(&search_text, Style::default().fg(Color::Yellow)),
        Span::styled("]", Style::default().fg(Color::DarkGray)),
    ]))
    .block(
        Block::default()
            .title(" IDOS Log Viewer ")
            .borders(Borders::ALL),
    );
    f.render_widget(header, chunks[0]);

    // --- Log area ---
    let log_area = chunks[1];
    let viewport_height = log_area.height.saturating_sub(2) as usize; // account for borders

    let visible = app.visible_indices();
    let visible_count = visible.len();

    // Auto-scroll: pin to bottom
    if app.auto_scroll && visible_count > viewport_height {
        app.scroll_offset = visible_count - viewport_height;
    }

    // If current search match is visible, scroll to it
    if !search_matches.is_empty() && !app.auto_scroll {
        let current_entry_idx = search_matches[app.search_match_index % search_matches.len()];
        // Find position of this entry in the visible list
        if let Some(vis_pos) = visible.iter().position(|&i| i == current_entry_idx) {
            if vis_pos < app.scroll_offset {
                app.scroll_offset = vis_pos;
            } else if vis_pos >= app.scroll_offset + viewport_height {
                app.scroll_offset = vis_pos - viewport_height + 1;
            }
        }
    }

    let current_match_entry = if !search_matches.is_empty() {
        Some(search_matches[app.search_match_index % search_matches.len()])
    } else {
        None
    };

    let start = app.start_time;
    let lines: Vec<Line> = visible
        .iter()
        .skip(app.scroll_offset)
        .take(viewport_height)
        .map(|&idx| {
            let entry = &app.entries[idx];

            // Format elapsed time as hh:mm:ss.s
            let elapsed = entry.timestamp.duration_since(start);
            let total_secs = elapsed.as_secs();
            let m = total_secs / 60;
            let s = total_secs % 60;
            let ms = elapsed.subsec_millis();
            let time_str = format!("{m:02}:{s:02}.{ms:03}");

            let tag_color = if entry.color > 0 {
                ansi_to_color(entry.color)
            } else {
                Color::White
            };

            let tag_display = if entry.tag.is_empty() {
                "        ".to_string()
            } else {
                format!("{:<8}", entry.tag)
            };

            let is_current_match = current_match_entry == Some(idx);
            let is_any_match = search_matches.contains(&idx);

            let msg_spans = if let Some(ref query) = app.search {
                highlight_matches(&entry.message, query, is_current_match, is_any_match)
            } else {
                vec![Span::raw(&entry.message)]
            };

            let mut spans = vec![
                Span::styled(time_str, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(tag_display, Style::default().fg(tag_color)),
                Span::styled(": ", Style::default().fg(Color::DarkGray)),
            ];
            spans.extend(msg_spans);

            Line::from(spans)
        })
        .collect();

    let scroll_indicator = if app.auto_scroll {
        " ▼ auto-scroll "
    } else {
        ""
    };

    let log_widget = Paragraph::new(lines).block(
        Block::default()
            .title(scroll_indicator)
            .borders(Borders::ALL),
    );
    f.render_widget(log_widget, log_area);

    // --- Input bar ---
    if show_input {
        let (prompt, color) = match app.mode {
            InputMode::FilterInput => ("Filter tag: ", Color::Cyan),
            InputMode::SearchInput => ("/", Color::Yellow),
            InputMode::Normal => unreachable!(),
        };

        let input = Paragraph::new(Line::from(vec![
            Span::styled(prompt, Style::default().fg(color)),
            Span::raw(&app.input_buf),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
        ]))
        .block(Block::default().borders(Borders::ALL));
        f.render_widget(input, chunks[2]);
    }
}

fn highlight_matches<'a>(
    text: &'a str,
    query: &str,
    is_current_match: bool,
    is_any_match: bool,
) -> Vec<Span<'a>> {
    if !is_any_match || query.is_empty() {
        return vec![Span::raw(text)];
    }

    let query_lower = query.to_ascii_lowercase();
    let text_lower = text.to_ascii_lowercase();
    let mut spans = Vec::new();
    let mut last = 0;

    let bg = if is_current_match {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let fg = Color::Black;

    for (start, _) in text_lower.match_indices(&query_lower) {
        if start > last {
            spans.push(Span::raw(&text[last..start]));
        }
        spans.push(Span::styled(
            &text[start..start + query.len()],
            Style::default().fg(fg).bg(bg),
        ));
        last = start + query.len();
    }
    if last < text.len() {
        spans.push(Span::raw(&text[last..]));
    }

    spans
}
