use std::time::Instant;

pub struct LogEntry {
    pub tag: String,
    pub color: u8,
    pub message: String,
    pub timestamp: Instant,
}

/// Parse a line in the TaggedLogger ANSI format:
/// `\x1b[{color}m{tag:8}\x1b[0m: {message}`
///
/// Lines that don't match this pattern are treated as untagged.
pub fn parse_log_line(line: &str, timestamp: Instant) -> LogEntry {
    // Try to match: ESC[{digits}m{8-char tag}ESC[0m: {message}
    if let Some(entry) = try_parse_tagged(line, timestamp) {
        return entry;
    }

    LogEntry {
        tag: String::new(),
        color: 0,
        message: line.to_string(),
        timestamp,
    }
}

fn try_parse_tagged(line: &str, timestamp: Instant) -> Option<LogEntry> {
    // Must start with ESC[
    let rest = line.strip_prefix("\x1b[")?;

    // Read color digits until 'm'
    let m_pos = rest.find('m')?;
    let color_str = &rest[..m_pos];
    let color: u8 = color_str.parse().ok()?;
    let rest = &rest[m_pos + 1..];

    // Next 8 characters are the tag
    if rest.len() < 8 {
        return None;
    }
    let tag = &rest[..8];
    let rest = &rest[8..];

    // Must be followed by ESC[0m
    let rest = rest.strip_prefix("\x1b[0m")?;

    // Must be followed by ": "
    let rest = rest.strip_prefix(": ")?;

    Some(LogEntry {
        tag: tag.trim_end().to_string(),
        color,
        message: rest.to_string(),
        timestamp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Instant {
        Instant::now()
    }

    #[test]
    fn parse_tagged_line() {
        let line = "\x1b[31mFATFS   \x1b[0m: Opening file CONFIG.SYS";
        let entry = parse_log_line(line, now());
        assert_eq!(entry.tag, "FATFS");
        assert_eq!(entry.color, 31);
        assert_eq!(entry.message, "Opening file CONFIG.SYS");
    }

    #[test]
    fn parse_untagged_line() {
        let line = "Some random output";
        let entry = parse_log_line(line, now());
        assert_eq!(entry.tag, "");
        assert_eq!(entry.color, 0);
        assert_eq!(entry.message, "Some random output");
    }

    #[test]
    fn parse_bright_color() {
        let line = "\x1b[93mKERNEL  \x1b[0m: Page fault at 0xDEAD";
        let entry = parse_log_line(line, now());
        assert_eq!(entry.tag, "KERNEL");
        assert_eq!(entry.color, 93);
        assert_eq!(entry.message, "Page fault at 0xDEAD");
    }
}
