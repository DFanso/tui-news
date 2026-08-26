#[derive(Debug, Clone)]
pub struct Feed {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub last_fetched: Option<i64>,
    pub error: Option<String>,
    pub unread: i64,
}

#[derive(Debug, Clone)]
pub struct Article {
    pub id: i64,
    pub feed_title: String,
    pub title: String,
    pub url: Option<String>,
    pub published: Option<i64>,
    pub summary: Option<String>,
    pub content: Option<String>,
    pub is_read: bool,
}

impl Article {
    pub fn body_html(&self) -> &str {
        self.content
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .or(self.summary.as_deref())
            .unwrap_or("")
    }
}

#[derive(Debug, Clone)]
pub struct NewArticle {
    pub guid: String,
    pub title: String,
    pub url: Option<String>,
    pub published: Option<i64>,
    pub summary: Option<String>,
    pub content: Option<String>,
}

pub const DEFAULT_FEEDS: &[(&str, &str)] = &[
    ("https://news.ycombinator.com/rss", "Hacker News"),
    ("https://feeds.bbci.co.uk/news/world/rss.xml", "BBC World"),
    ("https://blog.rust-lang.org/feed.xml", "Rust Blog"),
    ("https://lobste.rs/rss", "Lobsters"),
    ("https://www.theverge.com/rss/index.xml", "The Verge"),
];
