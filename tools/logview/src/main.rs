mod app;
mod parse;
mod reader;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use app::{App, InputMode};
use reader::Source;

fn main() {
    let source = parse_args();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    rt.block_on(run(source)).expect("Application error");
}

fn parse_args() -> Source {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" => {
                i += 1;
                return Source::Socket(args.get(i).expect("--socket requires a path").clone());
            }
            "--help" | "-h" => {
                eprintln!("Usage: logview [--socket <path>]");
                eprintln!();
                eprintln!("Reads IDOS serial log output and displays it in a filterable TUI.");
                eprintln!();
                eprintln!("  --socket <path>  Connect to a Unix socket instead of reading stdin");
                eprintln!();
                eprintln!("Pipe usage: qemu ... -serial stdio 2>&1 | logview");
                std::process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {other}");
                std::process::exit(1);
            }
        }
    }
    Source::Stdin
}

async fn run(source: Source) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let (tx, mut rx) = mpsc::unbounded_channel();

    tokio::task::spawn(async move {
        reader::spawn_reader(source, tx).await;
    });

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Poll for crossterm events or new log entries
        let timeout = Duration::from_millis(50);
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match app.mode {
                    InputMode::Normal => handle_normal_key(&mut app, key.code, key.modifiers),
                    InputMode::FilterInput => handle_input_key(&mut app, key.code, true),
                    InputMode::SearchInput => handle_input_key(&mut app, key.code, false),
                }
            }
        }

        // Drain all available log entries
        while let Ok(entry) = rx.try_recv() {
            app.entries.push(entry);
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn handle_normal_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    let viewport_height = 20; // approximate; real height comes from render

    match code {
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('f') => {
            app.mode = InputMode::FilterInput;
            app.input_buf.clear();
        }
        KeyCode::Char('F') => {
            app.filter = None;
            app.scroll_offset = 0;
            app.auto_scroll = true;
        }
        KeyCode::Char('/') => {
            app.mode = InputMode::SearchInput;
            app.input_buf.clear();
        }
        KeyCode::Esc => {
            app.search = None;
            app.search_match_index = 0;
        }
        KeyCode::Char('n') => app.next_match(),
        KeyCode::Char('N') => app.prev_match(),
        KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
        KeyCode::Down | KeyCode::Char('j') => {
            let visible_count = app.visible_indices().len();
            app.scroll_down(visible_count, viewport_height);
        }
        KeyCode::Char('g') => app.jump_to_top(),
        KeyCode::Char('G') => {
            let visible_count = app.visible_indices().len();
            app.jump_to_bottom(visible_count, viewport_height);
        }
        _ => {}
    }
}

fn handle_input_key(app: &mut App, code: KeyCode, is_filter: bool) {
    match code {
        KeyCode::Enter => {
            if is_filter {
                app.submit_filter();
            } else {
                app.submit_search();
            }
        }
        KeyCode::Esc => {
            app.input_buf.clear();
            app.mode = InputMode::Normal;
        }
        KeyCode::Backspace => {
            app.input_buf.pop();
        }
        KeyCode::Char(c) => {
            app.input_buf.push(c);
        }
        _ => {}
    }
}
