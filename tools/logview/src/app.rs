use std::time::Instant;

use crate::parse::LogEntry;

#[derive(PartialEq)]
pub enum InputMode {
    Normal,
    FilterInput,
    SearchInput,
}

pub struct App {
    pub start_time: Instant,
    pub entries: Vec<LogEntry>,
    pub filter: Option<String>,
    pub search: Option<String>,
    pub search_match_index: usize,
    pub scroll_offset: usize,
    pub auto_scroll: bool,
    pub mode: InputMode,
    pub input_buf: String,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            entries: Vec::new(),
            filter: None,
            search: None,
            search_match_index: 0,
            scroll_offset: 0,
            auto_scroll: true,
            mode: InputMode::Normal,
            input_buf: String::new(),
            should_quit: false,
        }
    }

    /// Returns indices into `self.entries` that pass the current tag filter.
    pub fn visible_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if let Some(ref f) = self.filter {
                    e.tag.eq_ignore_ascii_case(f)
                } else {
                    true
                }
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Returns indices (into `self.entries`) of visible entries that match the search.
    pub fn search_match_indices(&self) -> Vec<usize> {
        let Some(ref query) = self.search else {
            return Vec::new();
        };
        let query_lower = query.to_ascii_lowercase();
        self.visible_indices()
            .into_iter()
            .filter(|&i| self.entries[i].message.to_ascii_lowercase().contains(&query_lower))
            .collect()
    }

    pub fn submit_filter(&mut self) {
        let tag = self.input_buf.trim().to_string();
        self.filter = if tag.is_empty() { None } else { Some(tag) };
        self.input_buf.clear();
        self.mode = InputMode::Normal;
        self.scroll_offset = 0;
        self.auto_scroll = true;
    }

    pub fn submit_search(&mut self) {
        let query = self.input_buf.trim().to_string();
        self.search = if query.is_empty() { None } else { Some(query) };
        self.search_match_index = 0;
        self.input_buf.clear();
        self.mode = InputMode::Normal;
    }

    pub fn next_match(&mut self) {
        let matches = self.search_match_indices();
        if matches.is_empty() {
            return;
        }
        self.search_match_index = (self.search_match_index + 1) % matches.len();
        self.auto_scroll = false;
    }

    pub fn prev_match(&mut self) {
        let matches = self.search_match_indices();
        if matches.is_empty() {
            return;
        }
        self.search_match_index = if self.search_match_index == 0 {
            matches.len() - 1
        } else {
            self.search_match_index - 1
        };
        self.auto_scroll = false;
    }

    pub fn scroll_up(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self, visible_count: usize, viewport_height: usize) {
        self.auto_scroll = false;
        if visible_count > viewport_height {
            self.scroll_offset = (self.scroll_offset + 1).min(visible_count - viewport_height);
        }
    }

    pub fn jump_to_top(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = 0;
    }

    pub fn jump_to_bottom(&mut self, visible_count: usize, viewport_height: usize) {
        self.auto_scroll = true;
        if visible_count > viewport_height {
            self.scroll_offset = visible_count - viewport_height;
        } else {
            self.scroll_offset = 0;
        }
    }
}
