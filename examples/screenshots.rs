//! Render the real TUI into PNG screenshots for the README.
//!
//! ```text
//! cargo run --example screenshots
//! ```

use anyhow::{Context, Result};
use fontdue::Font;
use image::{Rgb, RgbImage};
use ratatui::backend::TestBackend;
use ratatui::layout::Position;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tui_news::app::{App, Focus, Mode};
use tui_news::model::NewArticle;
use tui_news::ui;

const COLS: u16 = 120;
const ROWS: u16 = 36;
const CELL_W: u32 = 9;
const CELL_H: u32 = 18;
const FONT_SIZE: f32 = 16.0;

fn main() -> Result<()> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots");
    std::fs::create_dir_all(&out)?;

    let dir = tempfile::tempdir()?;
    let db = dir.path().join("demo.db");
    let (tx, _rx) = mpsc::unbounded_channel();
    let mut app = App::open(db, tx)?;
    seed_demo(&mut app)?;

    app.status.clear();
    app.status_error = false;
    app.focus = Focus::Articles;
    capture(&mut app, out.join("main.png"))?;

    app.mode = Mode::AddFeed;
    app.input = "tech".into();
    app.catalog_state.select(Some(0));
    capture(&mut app, out.join("add-feed.png"))?;

    app.mode = Mode::Help;
    app.input.clear();
    capture(&mut app, out.join("help.png"))?;

    println!("wrote screenshots to {}", out.display());
    Ok(())
}

fn seed_demo(app: &mut App) -> Result<()> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let samples: &[(&str, &str, &str)] = &[
        (
            "Rust 1.85.0 is out",
            "https://blog.rust-lang.org/2026/08/01/1.85.0",
            "<p>The Rust team is happy to announce a new version of the language.</p>\
             <p>This release includes <strong>faster compiles</strong>, better diagnostics, \
             and a handful of <a href=\"https://blog.rust-lang.org\">library additions</a>.</p>\
             <ul><li>Improved async errors</li><li>New Clippy lints</li><li>Stabilized APIs</li></ul>\
             <p>Upgrade with <code>rustup update stable</code> and report issues on GitHub.</p>",
        ),
        (
            "Senate vote delayed until Thursday",
            "https://www.bbc.com/news/world-123",
            "<p>Negotiators spent the night rewriting the bill after a late objection.</p>\
             <p><em>Officials said</em> talks would resume in the morning.</p>",
        ),
        (
            "New chip foundry breaks ground in Arizona",
            "https://arstechnica.com/foundry",
            "<p>Construction crews started work on a plant expected to produce 3nm wafers by 2028.</p>",
        ),
        (
            "Show HN: a tiny SQLite TUI",
            "https://news.ycombinator.com/item?id=1",
            "",
        ),
        (
            "Lobsters: terminal news readers",
            "https://lobste.rs/s/news",
            "",
        ),
    ];

    let feeds = app.feeds.clone();
    for (i, feed) in feeds.iter().enumerate() {
        let mut batch = Vec::new();
        for (offset, (title, url, html)) in samples.iter().enumerate() {
            let content = if html.is_empty() {
                None
            } else {
                Some((*html).to_string())
            };
            batch.push(NewArticle {
                guid: format!("{}-{}", feed.id, offset),
                title: (*title).to_string(),
                url: Some((*url).to_string()),
                published: Some(now - ((i + offset) as i64) * 3600),
                summary: content
                    .as_ref()
                    .map(|s| s.chars().take(80).collect::<String>()),
                content,
            });
        }
        app.store.upsert_articles(feed.id, &batch)?;
    }
    app.reload()?;
    if !app.articles.is_empty() {
        app.article_state.select(Some(0));
    }
    Ok(())
}

fn capture(app: &mut App, path: PathBuf) -> Result<()> {
    let backend = TestBackend::new(COLS, ROWS);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::draw(frame, app))?;
    let buffer = terminal.backend().buffer().clone();
    let img = rasterize(&buffer)?;
    img.save(&path)
        .with_context(|| format!("saving {}", path.display()))?;
    println!("  {}", path.display());
    Ok(())
}

fn rasterize(buffer: &ratatui::buffer::Buffer) -> Result<RgbImage> {
    let font_path = first_font()?;
    let font = Font::from_bytes(std::fs::read(font_path)?, fontdue::FontSettings::default())
        .map_err(|e| anyhow::anyhow!("font: {e}"))?;

    let area = buffer.area;
    let mut img = RgbImage::from_pixel(
        u32::from(area.width) * CELL_W,
        u32::from(area.height) * CELL_H,
        Rgb([40, 44, 52]),
    );

    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buffer[Position::new(x, y)];
            let mut fg = map_color(cell.fg, false);
            let mut bg = map_color(cell.bg, true);
            if cell.modifier.contains(Modifier::REVERSED) {
                std::mem::swap(&mut fg, &mut bg);
            }
            fill_cell(&mut img, x, y, bg);

            let ch = cell.symbol().chars().next().unwrap_or(' ');
            if ch != ' ' {
                blit_glyph(&mut img, &font, ch, x, y, fg);
            }
            if cell.modifier.contains(Modifier::UNDERLINED) {
                let base_y = u32::from(y) * CELL_H + CELL_H - 3;
                let base_x = u32::from(x) * CELL_W;
                for dx in 0..CELL_W {
                    img.put_pixel(base_x + dx, base_y, Rgb(fg));
                }
            }
        }
    }
    Ok(img)
}

fn fill_cell(img: &mut RgbImage, x: u16, y: u16, rgb: [u8; 3]) {
    let x0 = u32::from(x) * CELL_W;
    let y0 = u32::from(y) * CELL_H;
    for dy in 0..CELL_H {
        for dx in 0..CELL_W {
            img.put_pixel(x0 + dx, y0 + dy, Rgb(rgb));
        }
    }
}

fn blit_glyph(img: &mut RgbImage, font: &Font, ch: char, x: u16, y: u16, fg: [u8; 3]) {
    let (metrics, bitmap) = font.rasterize(ch, FONT_SIZE);
    let x0 = i32::from(x) * CELL_W as i32 + 1 + metrics.xmin;
    let y0 = i32::from(y) * CELL_H as i32 + 14 - metrics.height as i32 - metrics.ymin;
    for row in 0..metrics.height {
        for col in 0..metrics.width {
            let alpha = bitmap[row * metrics.width + col];
            if alpha == 0 {
                continue;
            }
            let px = x0 + col as i32;
            let py = y0 + row as i32;
            if px < 0 || py < 0 {
                continue;
            }
            let px = px as u32;
            let py = py as u32;
            if px >= img.width() || py >= img.height() {
                continue;
            }
            let dest = img.get_pixel(px, py).0;
            let a = alpha as u16;
            let blend = |c: u8, d: u8| ((c as u16 * a + d as u16 * (255 - a)) / 255) as u8;
            img.put_pixel(
                px,
                py,
                Rgb([blend(fg[0], dest[0]), blend(fg[1], dest[1]), blend(fg[2], dest[2])]),
            );
        }
    }
}

fn map_color(color: Color, background: bool) -> [u8; 3] {
    match color {
        Color::Reset | Color::Indexed(0) if background => [40, 44, 52],
        Color::Reset => [220, 223, 228],
        Color::Black => [40, 44, 52],
        Color::White => [220, 223, 228],
        Color::Gray => [92, 99, 112],
        Color::DarkGray => [62, 68, 81],
        Color::Red => [224, 108, 117],
        Color::Green => [152, 195, 121],
        Color::Yellow => [229, 192, 123],
        Color::Blue => [97, 175, 239],
        Color::Magenta => [198, 120, 221],
        Color::Cyan => [86, 182, 194],
        Color::LightRed => [224, 108, 117],
        Color::LightGreen => [152, 195, 121],
        Color::LightYellow => [229, 192, 123],
        Color::LightBlue => [97, 175, 239],
        Color::LightMagenta => [198, 120, 221],
        Color::LightCyan => [86, 182, 194],
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Indexed(n) => indexed(n),
    }
}

fn indexed(n: u8) -> [u8; 3] {
    match n {
        0 => [40, 44, 52],
        1 => [224, 108, 117],
        2 => [152, 195, 121],
        3 => [229, 192, 123],
        4 => [97, 175, 239],
        5 => [198, 120, 221],
        6 => [86, 182, 194],
        7 => [171, 178, 191],
        _ => [220, 223, 228],
    }
}

fn first_font() -> Result<&'static str> {
    [
        r"C:\Windows\Fonts\CascadiaMono.ttf",
        r"C:\Windows\Fonts\consola.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/System/Library/Fonts/Menlo.ttc",
    ]
    .into_iter()
    .find(|path| Path::new(path).exists())
    .context("no monospace font found")
}
