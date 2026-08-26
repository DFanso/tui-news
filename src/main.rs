use anyhow::Result;
use tui_news::app;
use tui_news::db;
use tui_news::ui;
use clap::Parser;
use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(
    name = "tui-news",
    version,
    about = "A keyboard-first RSS/Atom news reader for the terminal"
)]
struct Cli {
    /// Path to the SQLite database (defaults to the platform data directory)
    #[arg(long)]
    db: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_path = match cli.db {
        Some(path) => path,
        None => db::default_db_path()?,
    };

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, db_path).await;
    ratatui::restore();
    if let Err(error) = &result {
        eprintln!("tui-news: {error:#}");
    }
    result
}

async fn run(terminal: &mut ratatui::DefaultTerminal, db_path: PathBuf) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut app = app::App::open(db_path, tx)?;
    if app.needs_initial_fetch() {
        app.refresh_all();
    }

    let mut events = EventStream::new();
    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;
        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(event)) => match event {
                        Event::Key(key) if app.handle_key(key)? => break,
                        Event::Key(_) | Event::Resize(_, _) => {}
                        _ => {}
                    },
                    Some(Err(error)) => anyhow::bail!("event stream: {error}"),
                    None => break,
                }
            }
            Some(msg) = rx.recv() => {
                app.apply_fetch(msg)?;
            }
        }
    }
    Ok(())
}
