use crate::model::{Article, DEFAULT_FEEDS, Feed, NewArticle};
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening database {}", path.display()))?;
        Self::from_conn(conn)
    }

    fn from_conn(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS feeds (
                id INTEGER PRIMARY KEY,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                site_url TEXT,
                last_fetched INTEGER,
                error TEXT
            );
            CREATE TABLE IF NOT EXISTS articles (
                id INTEGER PRIMARY KEY,
                feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
                guid TEXT NOT NULL,
                title TEXT NOT NULL,
                url TEXT,
                published INTEGER,
                summary TEXT,
                content TEXT,
                is_read INTEGER NOT NULL DEFAULT 0,
                UNIQUE(feed_id, guid)
            );
            CREATE INDEX IF NOT EXISTS idx_articles_feed_published
                ON articles(feed_id, published DESC);
            CREATE INDEX IF NOT EXISTS idx_articles_read ON articles(is_read);
            ",
        )?;
        Ok(Self { conn })
    }

    /// Insert starter feeds when the database is empty. Returns true if seeded.
    pub fn seed_defaults_if_empty(&self) -> Result<bool> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM feeds", [], |row| row.get(0))?;
        if count > 0 {
            return Ok(false);
        }
        for (url, title) in DEFAULT_FEEDS {
            self.conn.execute(
                "INSERT INTO feeds (url, title) VALUES (?1, ?2)",
                params![url, title],
            )?;
        }
        Ok(true)
    }

    pub fn list_feeds(&self) -> Result<Vec<Feed>> {
        let mut unread = HashMap::new();
        let mut stmt = self
            .conn
            .prepare("SELECT feed_id, COUNT(*) FROM articles WHERE is_read = 0 GROUP BY feed_id")?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
        for row in rows {
            let (id, count) = row?;
            unread.insert(id, count);
        }

        let mut stmt = self.conn.prepare(
            "SELECT id, url, title, last_fetched, error FROM feeds ORDER BY title COLLATE NOCASE",
        )?;
        let feeds = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                Ok(Feed {
                    id,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    last_fetched: row.get(3)?,
                    error: row.get(4)?,
                    unread: unread.get(&id).copied().unwrap_or(0),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(feeds)
    }

    pub fn add_feed(&self, url: &str, title: &str) -> Result<Feed> {
        self.conn.execute(
            "INSERT INTO feeds (url, title) VALUES (?1, ?2)",
            params![url, title],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(Feed {
            id,
            url: url.to_string(),
            title: title.to_string(),
            last_fetched: None,
            error: None,
            unread: 0,
        })
    }

    pub fn feed_by_url(&self, url: &str) -> Result<Option<Feed>> {
        self.conn
            .query_row(
                "SELECT id, url, title, last_fetched, error FROM feeds WHERE url = ?1",
                params![url],
                |row| {
                    Ok(Feed {
                        id: row.get(0)?,
                        url: row.get(1)?,
                        title: row.get(2)?,
                        last_fetched: row.get(3)?,
                        error: row.get(4)?,
                        unread: 0,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update_feed_ok(
        &self,
        id: i64,
        title: &str,
        site_url: Option<&str>,
        now: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE feeds SET title = ?1, site_url = ?2, last_fetched = ?3, error = NULL WHERE id = ?4",
            params![title, site_url, now, id],
        )?;
        Ok(())
    }

    pub fn update_feed_error(&self, id: i64, error: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE feeds SET last_fetched = ?1, error = ?2 WHERE id = ?3",
            params![now, error, id],
        )?;
        Ok(())
    }

    pub fn delete_feed(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM feeds WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn feed_exists(&self, id: i64) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM feeds WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(n > 0)
    }

    /// Insert or update articles. Returns how many rows were newly inserted.
    pub fn upsert_articles(&self, feed_id: i64, articles: &[NewArticle]) -> Result<u64> {
        if articles.is_empty() {
            return Ok(0);
        }
        let before: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE feed_id = ?1",
            params![feed_id],
            |row| row.get(0),
        )?;
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "
                INSERT INTO articles (feed_id, guid, title, url, published, summary, content, is_read)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)
                ON CONFLICT(feed_id, guid) DO UPDATE SET
                    title = excluded.title,
                    url = excluded.url,
                    published = excluded.published,
                    summary = excluded.summary,
                    content = excluded.content
                ",
            )?;
            for article in articles {
                stmt.execute(params![
                    feed_id,
                    article.guid,
                    article.title,
                    article.url,
                    article.published,
                    article.summary,
                    article.content,
                ])?;
            }
        }
        tx.commit()?;
        let after: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM articles WHERE feed_id = ?1",
            params![feed_id],
            |row| row.get(0),
        )?;
        Ok(after.saturating_sub(before) as u64)
    }

    pub fn list_articles(
        &self,
        feed_id: Option<i64>,
        query: Option<&str>,
        unread_only: bool,
    ) -> Result<Vec<Article>> {
        let mut sql = String::from(
            "
            SELECT a.id, f.title, a.title, a.url,
                   a.published, a.summary, a.content, a.is_read
            FROM articles a
            JOIN feeds f ON f.id = a.feed_id
            WHERE 1=1
            ",
        );
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(id) = feed_id {
            sql.push_str(" AND a.feed_id = ?");
            args.push(id.into());
        }
        if let Some(q) = query.map(str::trim).filter(|s| !s.is_empty()) {
            sql.push_str(" AND (a.title LIKE ? OR IFNULL(a.summary, '') LIKE ?)");
            let pat = format!("%{q}%");
            args.push(pat.clone().into());
            args.push(pat.into());
        }
        if unread_only {
            sql.push_str(" AND a.is_read = 0");
        }
        sql.push_str(" ORDER BY COALESCE(a.published, 0) DESC, a.id DESC LIMIT 500");

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), |row| {
            Ok(Article {
                id: row.get(0)?,
                feed_title: row.get(1)?,
                title: row.get(2)?,
                url: row.get(3)?,
                published: row.get(4)?,
                summary: row.get(5)?,
                content: row.get(6)?,
                is_read: row.get::<_, i64>(7)? != 0,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn set_read(&self, id: i64, read: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE articles SET is_read = ?1 WHERE id = ?2",
            params![if read { 1 } else { 0 }, id],
        )?;
        Ok(())
    }

    pub fn mark_feed_read(&self, feed_id: Option<i64>) -> Result<()> {
        match feed_id {
            Some(id) => {
                self.conn.execute(
                    "UPDATE articles SET is_read = 1 WHERE feed_id = ?1",
                    params![id],
                )?;
            }
            None => {
                self.conn.execute("UPDATE articles SET is_read = 1", [])?;
            }
        }
        Ok(())
    }
}

pub fn default_db_path() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("could not find a user data directory"))?
        .join("tui-news");
    Ok(dir.join("tui-news.db"))
}

#[cfg(test)]
impl Store {
    pub fn memory() -> Result<Self> {
        Self::from_conn(Connection::open_in_memory()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(guid: &str, title: &str) -> NewArticle {
        NewArticle {
            guid: guid.into(),
            title: title.into(),
            url: Some(format!("https://example.com/{guid}")),
            published: Some(1_700_000_000),
            summary: Some("hello".into()),
            content: Some("<p>body</p>".into()),
        }
    }

    #[test]
    fn seed_then_skip() {
        let store = Store::memory().unwrap();
        assert!(store.seed_defaults_if_empty().unwrap());
        assert!(!store.seed_defaults_if_empty().unwrap());
        assert_eq!(store.list_feeds().unwrap().len(), DEFAULT_FEEDS.len());
    }

    #[test]
    fn upsert_counts_only_new_rows() {
        let store = Store::memory().unwrap();
        let feed = store
            .add_feed("https://example.com/rss", "Example")
            .unwrap();
        let first = store
            .upsert_articles(feed.id, &[sample("a", "One"), sample("b", "Two")])
            .unwrap();
        assert_eq!(first, 2);
        let second = store
            .upsert_articles(feed.id, &[sample("b", "Two updated"), sample("c", "Three")])
            .unwrap();
        assert_eq!(second, 1);
        let articles = store.list_articles(Some(feed.id), None, false).unwrap();
        assert_eq!(articles.len(), 3);
        let two = articles
            .iter()
            .find(|a| a.title.starts_with("Two"))
            .unwrap();
        assert_eq!(two.title, "Two updated");
        assert!(!two.is_read);
    }

    #[test]
    fn mark_read_and_unread_counts() {
        let store = Store::memory().unwrap();
        let feed = store
            .add_feed("https://example.com/rss", "Example")
            .unwrap();
        store
            .upsert_articles(feed.id, &[sample("a", "One"), sample("b", "Two")])
            .unwrap();
        assert_eq!(store.list_articles(None, None, true).unwrap().len(), 2);
        let articles = store.list_articles(Some(feed.id), None, false).unwrap();
        store.set_read(articles[0].id, true).unwrap();
        assert_eq!(store.list_articles(None, None, true).unwrap().len(), 1);
        let unread = store.list_articles(Some(feed.id), None, true).unwrap();
        assert_eq!(unread.len(), 1);
        store.mark_feed_read(Some(feed.id)).unwrap();
        assert_eq!(store.list_articles(None, None, true).unwrap().len(), 0);
    }

    #[test]
    fn delete_feed_cascades_articles() {
        let store = Store::memory().unwrap();
        let feed = store
            .add_feed("https://example.com/rss", "Example")
            .unwrap();
        store
            .upsert_articles(feed.id, &[sample("a", "One")])
            .unwrap();
        store.delete_feed(feed.id).unwrap();
        assert!(store.list_feeds().unwrap().is_empty());
        assert!(store.list_articles(None, None, false).unwrap().is_empty());
        assert!(!store.feed_exists(feed.id).unwrap());
    }

    #[test]
    fn search_matches_title() {
        let store = Store::memory().unwrap();
        let feed = store
            .add_feed("https://example.com/rss", "Example")
            .unwrap();
        store
            .upsert_articles(
                feed.id,
                &[sample("a", "Rust release"), sample("b", "Python notes")],
            )
            .unwrap();
        let hits = store.list_articles(None, Some("rust"), false).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Rust release");
    }
}
