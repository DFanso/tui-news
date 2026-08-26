# tui-news

A keyboard-first RSS/Atom news reader for the terminal.

Three panes — feeds, stories, reader — with vim keys, unread tracking, and a local SQLite store. First launch seeds a few public feeds so there is something to read immediately.

```
 TUI-NEWS   WED 26 AUG 2026  14:32   12 UNREAD
┌ FEEDS ─────────────┐┌ STORIES ─────────────────────────────────────┐
│▸ All            12 ││ ● Senate vote delayed                   14m  │
│  ● Hacker News   5 ││ ○ Rust 1.95 released                     2h  │
│    BBC World     7 ││ ● New foundry in Arizona                 3h  │
└────────────────────┘└──────────────────────────────────────────────┘
┌ STORY ─────────────────────────────────────────────────────────────┐
│ Senate vote delayed                                                │
│ BBC World  ·  14m  ·  unread                                       │
│                                                                    │
│ Wrapped article text…                                              │
└────────────────────────────────────────────────────────────────────┘
 j/k move  tab pane  r refresh  o open  space read  / find  q quit
```

## Install

Needs a Rust toolchain (1.85+) and a C compiler for bundled SQLite.

```bash
cargo install --git https://github.com/DFanso/tui-news
```

Or from a local clone:

```bash
cargo install --path .
```

## Usage

```bash
tui-news
tui-news --db ./mine.db
```

On first run the app creates a database and subscribes to Hacker News, BBC World, the Rust blog, Lobsters, and The Verge, then fetches them in the background.

Data lives in the platform user-data directory:

| OS | Path |
| --- | --- |
| Windows | `%APPDATA%\tui-news\tui-news.db` |
| macOS | `~/Library/Application Support/tui-news/tui-news.db` |
| Linux | `~/.local/share/tui-news/tui-news.db` |

## Keys

| Key | Action |
| --- | --- |
| `j` `k` / arrows | Move in the focused pane |
| `tab` `h` `l` | Cycle feeds → stories → reader |
| `enter` / `o` | Open the story in a browser (marks read) |
| `space` / `m` | Toggle read |
| `n` / `p` | Next / previous unread |
| `r` / `R` | Refresh selected feed / all feeds |
| `a` | Add a feed URL |
| `d` | Delete the selected feed |
| `/` | Search titles |
| `u` | Unread-only filter |
| `A` | Mark visible stories read |
| `?` | Key reference |
| `q` | Quit |

RSS, Atom, and JSON Feed are all accepted. HTML bodies are converted to wrapped terminal text.

## Development

```bash
cargo test
cargo run
```
