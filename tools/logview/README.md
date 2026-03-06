# logview — Kernel Log Viewer

TUI tool for filtering and searching IDOS kernel serial logs. Built with ratatui/crossterm.

Reads log output from stdin (piped from QEMU) or a Unix socket (`--socket <path>`).

## Usage

```
qemu ... -serial stdio 2>&1 | logview
logview --socket /tmp/idos-serial.sock
```

## Keys

- `f` — enter filter mode (show only matching lines)
- `F` — clear filter
- `/` — search, `n`/`N` — next/prev match
- `j`/`k` or arrows — scroll, `g`/`G` — top/bottom
- `q` or Ctrl-C — quit
