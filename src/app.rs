use crate::db::Store;
use crate::fetch::{self, FetchedFeed};
use crate::model::{Article, CatalogFeed, Feed, catalog_suggestions, looks_like_feed_url};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Feeds,
    Articles,
    Reader,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    AddFeed,
    Search,
    ConfirmDelete,
    Help,
}

#[derive(Debug)]
pub enum FetchMsg {
    Ok { feed_id: i64, fetched: FetchedFeed },
    Err { feed_id: i64, error: String },
}

pub struct App {
    pub store: Store,
    client: reqwest::Client,
    tx: UnboundedSender<FetchMsg>,
    pub feeds: Vec<Feed>,
    pub articles: Vec<Article>,
    pub feed_state: ListState,
    pub article_state: ListState,
    pub catalog_state: ListState,
    pub focus: Focus,
    pub mode: Mode,
    pub input: String,
    pub unread_only: bool,
    pub search: Option<String>,
    pub reader_scroll: u16,
    pub status: String,
    pub status_error: bool,
    pub in_flight: HashSet<i64>,
    pub seeded: bool,
    batch_new: u64,
}

impl App {
    pub fn open(db_path: PathBuf, tx: UnboundedSender<FetchMsg>) -> Result<Self> {
        let store = Store::open(&db_path)?;
        let seeded = store.seed_defaults_if_empty()?;
        let client = fetch::http_client()?;
        let mut app = Self {
            store,
            client,
            tx,
            feeds: Vec::new(),
            articles: Vec::new(),
            feed_state: ListState::default().with_selected(Some(0)),
            article_state: ListState::default(),
            catalog_state: ListState::default().with_selected(Some(0)),
            focus: Focus::Feeds,
            mode: Mode::Normal,
            input: String::new(),
            unread_only: false,
            search: None,
            reader_scroll: 0,
            status: if seeded {
                "starter feeds added — fetching…".into()
            } else {
                String::new()
            },
            status_error: false,
            in_flight: HashSet::new(),
            seeded,
            batch_new: 0,
        };
        app.reload()?;
        Ok(app)
    }

    pub fn needs_initial_fetch(&self) -> bool {
        self.seeded || self.feeds.iter().all(|f| f.last_fetched.is_none())
    }

    pub fn selected_feed_id(&self) -> Option<i64> {
        match self.feed_state.selected().unwrap_or(0) {
            0 => None,
            i => self.feeds.get(i - 1).map(|f| f.id),
        }
    }

    pub fn selected_feed(&self) -> Option<&Feed> {
        match self.feed_state.selected().unwrap_or(0) {
            0 => None,
            i => self.feeds.get(i - 1),
        }
    }

    pub fn selected_article(&self) -> Option<&Article> {
        self.article_state
            .selected()
            .and_then(|i| self.articles.get(i))
    }

    pub fn total_unread(&self) -> i64 {
        self.feeds.iter().map(|f| f.unread).sum()
    }

    pub fn fetching(&self) -> bool {
        !self.in_flight.is_empty()
    }

    pub fn reload(&mut self) -> Result<()> {
        let prev_article = self.selected_article().map(|a| a.id);
        let prev_feed_idx = self.feed_state.selected();
        self.feeds = self.store.list_feeds()?;
        if let Some(idx) = prev_feed_idx {
            if idx > self.feeds.len() {
                self.feed_state.select(Some(0));
            }
        }
        self.reload_articles(prev_article)?;
        Ok(())
    }

    fn reload_articles(&mut self, keep_id: Option<i64>) -> Result<()> {
        self.articles = self.store.list_articles(
            self.selected_feed_id(),
            self.search.as_deref(),
            self.unread_only,
        )?;
        let idx = keep_id
            .and_then(|id| self.articles.iter().position(|a| a.id == id))
            .or_else(|| (!self.articles.is_empty()).then_some(0));
        self.article_state.select(idx);
        self.reader_scroll = 0;
        Ok(())
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if key.kind != KeyEventKind::Press {
            return Ok(false);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            return Ok(true);
        }

        match self.mode {
            Mode::Help => {
                self.mode = Mode::Normal;
                return Ok(false);
            }
            Mode::AddFeed => return self.handle_add_feed(key),
            Mode::Search => return self.handle_search(key),
            Mode::ConfirmDelete => return self.handle_confirm_delete(key),
            Mode::Normal => {}
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Tab | KeyCode::BackTab => self.cycle_focus(key.code == KeyCode::Tab),
            KeyCode::Char('h') | KeyCode::Left => self.cycle_focus(false),
            KeyCode::Char('l') | KeyCode::Right => self.cycle_focus(true),
            KeyCode::Char('j') | KeyCode::Down => self.move_sel(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_sel(-1),
            KeyCode::Char('g') => self.jump(true),
            KeyCode::Char('G') => self.jump(false),
            KeyCode::PageDown => self.scroll_reader(8),
            KeyCode::PageUp => self.scroll_reader(-8),
            KeyCode::Char('r') => self.refresh_selected(),
            KeyCode::Char('R') => self.refresh_all(),
            KeyCode::Char('a') => self.open_add_feed(),
            KeyCode::Char('d') => {
                if self.selected_feed().is_some() {
                    self.mode = Mode::ConfirmDelete;
                } else {
                    self.set_status("select a feed to delete", true);
                }
            }
            KeyCode::Char('o') | KeyCode::Enter => self.open_browser()?,
            KeyCode::Char(' ') | KeyCode::Char('m') => self.toggle_read()?,
            KeyCode::Char('A') => self.mark_view_read()?,
            KeyCode::Char('n') => self.step_unread(1),
            KeyCode::Char('p') => self.step_unread(-1),
            KeyCode::Char('/') => {
                self.input = self.search.clone().unwrap_or_default();
                self.mode = Mode::Search;
            }
            KeyCode::Char('u') => {
                self.unread_only = !self.unread_only;
                let keep = self.selected_article().map(|a| a.id);
                self.reload_articles(keep)?;
                self.set_status(
                    if self.unread_only {
                        "showing unread only"
                    } else {
                        "showing all stories"
                    },
                    false,
                );
            }
            _ => {}
        }
        Ok(false)
    }

    fn open_add_feed(&mut self) {
        self.input.clear();
        self.catalog_state.select(Some(0));
        self.mode = Mode::AddFeed;
    }

    pub fn catalog_matches(&self) -> Vec<&'static CatalogFeed> {
        let subscribed: Vec<&str> = self.feeds.iter().map(|f| f.url.as_str()).collect();
        catalog_suggestions(&self.input, &subscribed)
    }

    fn handle_add_feed(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Up => self.move_catalog(-1),
            KeyCode::Down => self.move_catalog(1),
            KeyCode::Tab => {
                if let Some(feed) = self.selected_catalog() {
                    self.input = feed.url.to_string();
                    self.catalog_state.select(Some(0));
                }
            }
            KeyCode::Enter => {
                self.submit_add_feed()?;
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.catalog_state.select(Some(0));
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                self.catalog_state.select(Some(0));
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(c);
                self.catalog_state.select(Some(0));
            }
            _ => {}
        }
        Ok(false)
    }

    fn move_catalog(&mut self, delta: i32) {
        let len = self.catalog_matches().len();
        if len == 0 {
            self.catalog_state.select(None);
            return;
        }
        let cur = self.catalog_state.selected().unwrap_or(0);
        self.catalog_state.select(Some(wrap_index(cur, len, delta)));
    }

    fn selected_catalog(&self) -> Option<&'static CatalogFeed> {
        let matches = self.catalog_matches();
        self.catalog_state
            .selected()
            .and_then(|i| matches.get(i).copied())
    }

    fn submit_add_feed(&mut self) -> Result<()> {
        if looks_like_feed_url(&self.input) {
            let raw = self.input.clone();
            self.mode = Mode::Normal;
            self.input.clear();
            return self.add_feed_named(&raw, None);
        }
        if let Some(feed) = self.selected_catalog() {
            self.mode = Mode::Normal;
            self.input.clear();
            return self.add_feed_named(feed.url, Some(feed.name));
        }
        if !self.input.trim().is_empty() {
            let raw = self.input.clone();
            self.mode = Mode::Normal;
            self.input.clear();
            return self.add_feed_named(&raw, None);
        }
        self.set_status("pick a popular feed or paste an RSS URL", true);
        Ok(())
    }

    fn handle_search(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
                self.search = None;
                self.reload_articles(None)?;
            }
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                let q = self.input.trim().to_string();
                self.search = if q.is_empty() { None } else { Some(q) };
                self.reload_articles(None)?;
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.live_search()?;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                self.live_search()?;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(c);
                self.live_search()?;
            }
            _ => {}
        }
        Ok(false)
    }

    fn live_search(&mut self) -> Result<()> {
        let q = self.input.trim().to_string();
        self.search = if q.is_empty() { None } else { Some(q) };
        self.reload_articles(None)
    }

    fn handle_confirm_delete(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.mode = Mode::Normal;
                if let Some(feed) = self.selected_feed().cloned() {
                    self.store.delete_feed(feed.id)?;
                    self.set_status(&format!("removed {}", feed.title), false);
                    self.feed_state.select(Some(0));
                    self.reload()?;
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(false)
    }

    fn cycle_focus(&mut self, forward: bool) {
        self.focus = match (self.focus, forward) {
            (Focus::Feeds, true) | (Focus::Reader, false) => Focus::Articles,
            (Focus::Articles, true) | (Focus::Feeds, false) => Focus::Reader,
            (Focus::Reader, true) | (Focus::Articles, false) => Focus::Feeds,
        };
    }

    fn move_sel(&mut self, delta: i32) {
        match self.focus {
            Focus::Feeds => {
                let len = self.feeds.len() + 1;
                let cur = self.feed_state.selected().unwrap_or(0);
                let next = wrap_index(cur, len, delta);
                self.feed_state.select(Some(next));
                let _ = self.reload_articles(None);
            }
            Focus::Articles => {
                if self.articles.is_empty() {
                    return;
                }
                let cur = self.article_state.selected().unwrap_or(0);
                let next = wrap_index(cur, self.articles.len(), delta);
                self.article_state.select(Some(next));
                self.reader_scroll = 0;
            }
            Focus::Reader => self.scroll_reader(delta),
        }
    }

    fn jump(&mut self, top: bool) {
        match self.focus {
            Focus::Feeds => {
                let idx = if top { 0 } else { self.feeds.len() };
                self.feed_state.select(Some(idx));
                let _ = self.reload_articles(None);
            }
            Focus::Articles if !self.articles.is_empty() => {
                let idx = if top { 0 } else { self.articles.len() - 1 };
                self.article_state.select(Some(idx));
                self.reader_scroll = 0;
            }
            Focus::Reader => {
                self.reader_scroll = if top { 0 } else { u16::MAX / 2 };
            }
            _ => {}
        }
    }

    pub fn scroll_reader(&mut self, delta: i32) {
        if delta < 0 {
            self.reader_scroll = self.reader_scroll.saturating_sub((-delta) as u16);
        } else {
            self.reader_scroll = self.reader_scroll.saturating_add(delta as u16);
        }
    }

    pub fn clamp_reader_scroll(&mut self, max: u16) {
        if self.reader_scroll > max {
            self.reader_scroll = max;
        }
    }

    fn refresh_selected(&mut self) {
        match self.selected_feed().cloned() {
            Some(feed) => self.spawn_fetch(feed.id, feed.url),
            None => self.refresh_all(),
        }
    }

    pub fn refresh_all(&mut self) {
        let feeds: Vec<(i64, String)> = self.feeds.iter().map(|f| (f.id, f.url.clone())).collect();
        if feeds.is_empty() {
            self.set_status("no feeds to refresh — press a to add one", true);
            return;
        }
        self.batch_new = 0;
        self.set_status(&format!("fetching {} feeds…", feeds.len()), false);
        for (id, url) in feeds {
            self.spawn_fetch(id, url);
        }
    }

    fn spawn_fetch(&mut self, feed_id: i64, url: String) {
        if !self.in_flight.insert(feed_id) {
            return;
        }
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = fetch::fetch_feed(&client, &url).await;
            let msg = match result {
                Ok(fetched) => FetchMsg::Ok { feed_id, fetched },
                Err(e) => FetchMsg::Err {
                    feed_id,
                    error: e.to_string(),
                },
            };
            let _ = tx.send(msg);
        });
    }

    fn add_feed_named(&mut self, raw: &str, title: Option<&str>) -> Result<()> {
        let url = match fetch::normalize_url(raw) {
            Ok(url) => url,
            Err(e) => {
                self.set_status(&e.to_string(), true);
                return Ok(());
            }
        };
        if let Some(existing) = self.store.feed_by_url(&url)? {
            self.set_status(&format!("already subscribed: {}", existing.title), true);
            return Ok(());
        }
        let title = title.unwrap_or(url.as_str());
        let feed = self.store.add_feed(&url, title)?;
        self.set_status(&format!("added {title} — fetching…"), false);
        self.reload()?;
        if let Some(idx) = self.feeds.iter().position(|f| f.id == feed.id) {
            self.feed_state.select(Some(idx + 1));
        }
        self.spawn_fetch(feed.id, feed.url);
        Ok(())
    }

    fn toggle_read(&mut self) -> Result<()> {
        let Some(idx) = self.article_state.selected() else {
            return Ok(());
        };
        let Some(article) = self.articles.get(idx).cloned() else {
            return Ok(());
        };
        let next = !article.is_read;
        self.store.set_read(article.id, next)?;
        if let Some(article) = self.articles.get_mut(idx) {
            article.is_read = next;
        }
        self.feeds = self.store.list_feeds()?;
        Ok(())
    }

    fn mark_view_read(&mut self) -> Result<()> {
        self.store.mark_feed_read(self.selected_feed_id())?;
        let keep = self.selected_article().map(|a| a.id);
        self.reload()?;
        self.reload_articles(keep)?;
        self.set_status("marked visible stories read", false);
        Ok(())
    }

    fn open_browser(&mut self) -> Result<()> {
        let Some(article) = self.selected_article().cloned() else {
            self.set_status("no story selected", true);
            return Ok(());
        };
        let Some(url) = article.url.clone() else {
            self.set_status("story has no link", true);
            return Ok(());
        };
        if !article.is_read {
            self.store.set_read(article.id, true)?;
            if let Some(idx) = self.article_state.selected() {
                if let Some(a) = self.articles.get_mut(idx) {
                    a.is_read = true;
                }
            }
            self.feeds = self.store.list_feeds()?;
        }
        match open::that(&url) {
            Ok(()) => self.set_status("opened in browser", false),
            Err(e) => self.set_status(&format!("could not open browser: {e}"), true),
        }
        Ok(())
    }

    fn step_unread(&mut self, dir: i32) {
        let n = self.articles.len();
        if n == 0 {
            self.set_status("no stories", true);
            return;
        }
        let start = self.article_state.selected().unwrap_or(0);
        for step in 1..=n {
            let idx = if dir >= 0 {
                (start + step) % n
            } else {
                (start + n - (step % n)) % n
            };
            if !self.articles[idx].is_read {
                self.article_state.select(Some(idx));
                self.reader_scroll = 0;
                self.focus = Focus::Articles;
                return;
            }
        }
        self.set_status("no unread stories in this list", false);
    }

    pub fn apply_fetch(&mut self, msg: FetchMsg) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        match msg {
            FetchMsg::Ok { feed_id, fetched } => {
                self.in_flight.remove(&feed_id);
                let new_count = self.store.upsert_articles(feed_id, &fetched.articles)?;
                self.store.update_feed_ok(
                    feed_id,
                    &fetched.title,
                    fetched.site_url.as_deref(),
                    now,
                )?;
                self.batch_new += new_count;
            }
            FetchMsg::Err { feed_id, error } => {
                self.in_flight.remove(&feed_id);
                self.store.update_feed_error(feed_id, &error, now)?;
            }
        }

        let keep_article = self.selected_article().map(|a| a.id);
        let keep_feed = self.feed_state.selected();
        self.feeds = self.store.list_feeds()?;
        if let Some(idx) = keep_feed {
            if idx <= self.feeds.len() {
                self.feed_state.select(Some(idx));
            }
        }
        self.reload_articles(keep_article)?;

        if self.in_flight.is_empty() {
            self.set_status(
                &format!("refresh complete — {} new stories", self.batch_new),
                false,
            );
            self.batch_new = 0;
        } else {
            self.set_status(
                &format!(
                    "fetching {} remaining… ({} new so far)",
                    self.in_flight.len(),
                    self.batch_new
                ),
                false,
            );
        }
        Ok(())
    }

    fn set_status(&mut self, msg: &str, error: bool) {
        self.status = msg.to_string();
        self.status_error = error;
    }
}

fn wrap_index(current: usize, len: usize, delta: i32) -> usize {
    if len == 0 {
        return 0;
    }
    let len_i = len as i32;
    let next = current as i32 + delta;
    ((next % len_i + len_i) % len_i) as usize
}

#[cfg(test)]
mod tests {
    use super::wrap_index;

    #[test]
    fn wrap_moves_and_loops() {
        assert_eq!(wrap_index(0, 3, 1), 1);
        assert_eq!(wrap_index(2, 3, 1), 0);
        assert_eq!(wrap_index(0, 3, -1), 2);
    }
}
