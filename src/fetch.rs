use crate::html::one_line;
use crate::model::NewArticle;
use anyhow::{Result, anyhow};
use feed_rs::model::{Entry, Feed as ParsedFeed};
use feed_rs::parser;
use std::time::Duration;

const USER_AGENT: &str = concat!(
    "tui-news/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/DFanso/tui-news)"
);

#[derive(Debug, Clone)]
pub struct FetchedFeed {
    pub title: String,
    pub site_url: Option<String>,
    pub articles: Vec<NewArticle>,
}

pub fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(Into::into)
}

pub async fn fetch_feed(client: &reqwest::Client, url: &str) -> Result<FetchedFeed> {
    let response = client
        .get(url)
        .header(
            "Accept",
            "application/rss+xml, application/atom+xml, application/feed+json, application/json, application/xml, text/xml, */*",
        )
        .send()
        .await
        .map_err(|e| anyhow!("request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("HTTP {status}"));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| anyhow!("reading body: {e}"))?;
    let parsed = parser::parse(&bytes[..]).map_err(|e| anyhow!("parse error: {e}"))?;
    Ok(from_parsed(parsed, url))
}

pub fn from_parsed(parsed: ParsedFeed, source_url: &str) -> FetchedFeed {
    let title = parsed
        .title
        .as_ref()
        .map(|t| one_line(&t.content))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| source_url.to_string());
    let site_url = parsed
        .links
        .iter()
        .find(|link| {
            link.rel
                .as_deref()
                .is_none_or(|rel| rel == "alternate" || rel == "self")
        })
        .or(parsed.links.first())
        .map(|link| link.href.clone());
    let articles = parsed
        .entries
        .into_iter()
        .filter_map(entry_to_article)
        .collect();
    FetchedFeed {
        title,
        site_url,
        articles,
    }
}

fn entry_to_article(entry: Entry) -> Option<NewArticle> {
    let title = entry
        .title
        .map(|t| one_line(&t.content))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "(untitled)".into());
    let url = entry
        .links
        .iter()
        .find(|link| link.rel.as_deref().is_none_or(|rel| rel == "alternate"))
        .or(entry.links.first())
        .map(|link| link.href.clone());
    let guid = if entry.id.trim().is_empty() {
        url.clone().unwrap_or_else(|| title.clone())
    } else {
        entry.id
    };
    let published = entry.published.or(entry.updated).map(|dt| dt.timestamp());
    let summary = entry
        .summary
        .map(|t| t.content)
        .filter(|s| !s.trim().is_empty());
    let content = entry
        .content
        .and_then(|c| c.body)
        .filter(|s| !s.trim().is_empty());
    Some(NewArticle {
        guid,
        title,
        url,
        published,
        summary,
        content,
    })
}

pub fn normalize_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("feed URL is empty"));
    }
    let url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(anyhow!("URL must be http or https"));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example Feed</title>
    <link>https://example.com/</link>
    <item>
      <title>Hello &amp; welcome</title>
      <link>https://example.com/hello</link>
      <guid>hello-1</guid>
      <pubDate>Mon, 06 Sep 2010 00:01:00 +0000</pubDate>
      <description><![CDATA[<p>First <em>story</em>.</p>]]></description>
    </item>
    <item>
      <title></title>
      <link>https://example.com/empty</link>
      <guid>empty-1</guid>
    </item>
  </channel>
</rss>"#;

    #[test]
    fn parses_rss_items() {
        let parsed = parser::parse(RSS.as_bytes()).unwrap();
        let fetched = from_parsed(parsed, "https://example.com/rss");
        assert_eq!(fetched.title, "Example Feed");
        assert_eq!(fetched.site_url.as_deref(), Some("https://example.com/"));
        assert_eq!(fetched.articles.len(), 2);
        assert_eq!(fetched.articles[0].title, "Hello & welcome");
        assert_eq!(
            fetched.articles[0].url.as_deref(),
            Some("https://example.com/hello")
        );
        assert!(fetched.articles[0].published.is_some());
        assert_eq!(fetched.articles[1].title, "(untitled)");
    }

    #[test]
    fn normalize_adds_https() {
        assert_eq!(
            normalize_url("example.com/feed.xml").unwrap(),
            "https://example.com/feed.xml"
        );
        assert!(normalize_url("   ").is_err());
    }
}
