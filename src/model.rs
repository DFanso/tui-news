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
    /// Prefer the longest text the feed actually sent (full content, then
    /// description). Many RSS feeds only include a headline.
    pub fn body_html(&self) -> &str {
        crate::html::longest_html(self.content.as_deref(), self.summary.as_deref()).unwrap_or("")
    }

    pub fn has_body(&self) -> bool {
        !crate::html::looks_empty(self.body_html())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogFeed {
    pub name: &'static str,
    pub url: &'static str,
    pub kind: &'static str,
}

/// Popular public feeds the user can pick from in the add-feed dialog.
pub const CATALOG: &[CatalogFeed] = &[
    CatalogFeed {
        name: "Hacker News",
        url: "https://news.ycombinator.com/rss",
        kind: "tech",
    },
    CatalogFeed {
        name: "Lobsters",
        url: "https://lobste.rs/rss",
        kind: "tech",
    },
    CatalogFeed {
        name: "Rust Blog",
        url: "https://blog.rust-lang.org/feed.xml",
        kind: "tech",
    },
    CatalogFeed {
        name: "This Week in Rust",
        url: "https://this-week-in-rust.org/atom.xml",
        kind: "tech",
    },
    CatalogFeed {
        name: "LWN",
        url: "https://lwn.net/headlines/rss",
        kind: "tech",
    },
    CatalogFeed {
        name: "Ars Technica",
        url: "https://feeds.arstechnica.com/arstechnica/index",
        kind: "tech",
    },
    CatalogFeed {
        name: "The Verge",
        url: "https://www.theverge.com/rss/index.xml",
        kind: "tech",
    },
    CatalogFeed {
        name: "Wired",
        url: "https://www.wired.com/feed/rss",
        kind: "tech",
    },
    CatalogFeed {
        name: "BBC World",
        url: "https://feeds.bbci.co.uk/news/world/rss.xml",
        kind: "news",
    },
    CatalogFeed {
        name: "NPR News",
        url: "https://feeds.npr.org/1001/rss.xml",
        kind: "news",
    },
    CatalogFeed {
        name: "The Guardian World",
        url: "https://www.theguardian.com/world/rss",
        kind: "news",
    },
    CatalogFeed {
        name: "Al Jazeera",
        url: "https://www.aljazeera.com/xml/rss/all.xml",
        kind: "news",
    },
    CatalogFeed {
        name: "CNN Top",
        url: "https://rss.cnn.com/rss/edition.rss",
        kind: "news",
    },
    CatalogFeed {
        name: "NASA Breaking News",
        url: "https://www.nasa.gov/rss/dyn/breaking_news.rss",
        kind: "science",
    },
    CatalogFeed {
        name: "Krebs on Security",
        url: "https://krebsonsecurity.com/feed/",
        kind: "security",
    },
    CatalogFeed {
        name: "xkcd",
        url: "https://xkcd.com/rss.xml",
        kind: "comics",
    },
];

pub fn looks_like_feed_url(raw: &str) -> bool {
    let t = raw.trim();
    t.contains("://") || t.starts_with("www.") || (t.contains('.') && t.contains('/'))
}

pub fn catalog_suggestions(query: &str, subscribed_urls: &[&str]) -> Vec<&'static CatalogFeed> {
    let q = query.trim().to_lowercase();
    let searching = !q.is_empty() && !looks_like_feed_url(query);
    CATALOG
        .iter()
        .filter(|feed| {
            if subscribed_urls.contains(&feed.url) {
                return false;
            }
            if !searching {
                return true;
            }
            feed.name.to_lowercase().contains(&q)
                || feed.kind.to_lowercase().contains(&q)
                || feed.url.to_lowercase().contains(&q)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_hides_subscribed_and_filters() {
        let subscribed = ["https://news.ycombinator.com/rss"];
        let all = catalog_suggestions("", &subscribed);
        assert!(all.iter().all(|f| f.name != "Hacker News"));
        assert!(all.iter().any(|f| f.name == "NPR News"));

        let npr = catalog_suggestions("npr", &subscribed);
        assert_eq!(npr.len(), 1);
        assert_eq!(npr[0].name, "NPR News");
    }

    #[test]
    fn url_paste_is_not_treated_as_search() {
        let hits = catalog_suggestions("https://example.com/rss", &[]);
        assert_eq!(hits.len(), CATALOG.len());
        assert!(looks_like_feed_url("https://example.com/feed.xml"));
        assert!(!looks_like_feed_url("guardian"));
    }
}
