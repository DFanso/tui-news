use html2text::render::{RichAnnotation, TaggedLine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMark {
    Bold,
    Italic,
    Link,
    Code,
    Strike,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichSpan {
    pub text: String,
    pub marks: Vec<TextMark>,
}

/// Convert RSS/Atom HTML (or plain text) into wrapped terminal text.
#[cfg(test)]
pub fn html_to_text(html: &str, width: usize) -> String {
    html_to_rich(html, width)
        .into_iter()
        .map(|line| line.into_iter().map(|span| span.text).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// HTML to wrapped lines with bold/italic/link/code marks for the reader.
pub fn html_to_rich(html: &str, width: usize) -> Vec<Vec<RichSpan>> {
    let width = width.max(24);
    let trimmed = html.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    match html2text::config::rich()
        .allow_width_overflow()
        .lines_from_read(trimmed.as_bytes(), width)
    {
        Ok(lines) => lines.into_iter().map(line_to_spans).collect(),
        Err(_) => wrap_plain(&strip_tags(trimmed), width),
    }
}

fn line_to_spans(
    line: TaggedLine<Vec<RichAnnotation>>,
) -> Vec<RichSpan> {
    let spans: Vec<RichSpan> = line
        .tagged_strings()
        .map(|ts| RichSpan {
            text: ts.s.clone(),
            marks: ts.tag.iter().filter_map(mark_from).collect(),
        })
        .filter(|span| !span.text.is_empty())
        .collect();
    if spans.is_empty() {
        vec![RichSpan {
            text: String::new(),
            marks: Vec::new(),
        }]
    } else {
        spans
    }
}

fn mark_from(annotation: &RichAnnotation) -> Option<TextMark> {
    match annotation {
        RichAnnotation::Emphasis => Some(TextMark::Italic),
        RichAnnotation::Strong => Some(TextMark::Bold),
        RichAnnotation::Link(_) => Some(TextMark::Link),
        RichAnnotation::Code | RichAnnotation::Preformat(_) => Some(TextMark::Code),
        RichAnnotation::Strikeout => Some(TextMark::Strike),
        _ => None,
    }
}

fn wrap_plain(text: &str, width: usize) -> Vec<Vec<RichSpan>> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(vec![RichSpan {
                text: std::mem::take(&mut current),
                marks: Vec::new(),
            }]);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(vec![RichSpan {
            text: current,
            marks: Vec::new(),
        }]);
    }
    lines
}

/// Collapse HTML in titles and other one-line fields.
pub fn one_line(html: &str) -> String {
    let stripped = if html.contains('<') {
        strip_tags(html)
    } else {
        html.to_string()
    };
    decode_basic(&stripped)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_basic(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

/// Visible character count after tags are stripped. Used to pick the
/// longest RSS field (content vs description vs media).
pub fn visible_len(html: &str) -> usize {
    strip_tags(html).chars().count()
}

pub fn looks_empty(html: &str) -> bool {
    visible_len(html) < 12
}

pub fn longest_html<'a>(a: Option<&'a str>, b: Option<&'a str>) -> Option<&'a str> {
    match (
        a.filter(|s| !s.trim().is_empty()),
        b.filter(|s| !s.trim().is_empty()),
    ) {
        (Some(x), Some(y)) => Some(if visible_len(x) >= visible_len(y) {
            x
        } else {
            y
        }),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        _ => None,
    }
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_list_html() {
        let html = "<ul><li>Item one</li><li>Item two</li></ul>";
        let text = html_to_text(html, 40);
        assert!(text.contains("Item one"), "{text}");
        assert!(text.contains("Item two"), "{text}");
    }

    #[test]
    fn rich_marks_bold_and_links() {
        let html =
            r#"<p>Hello <strong>world</strong> and <a href="https://ex.com">a link</a>.</p>"#;
        let lines = html_to_rich(html, 60);
        let spans: Vec<&RichSpan> = lines.iter().flatten().collect();
        assert!(
            spans
                .iter()
                .any(|s| s.text.contains("world") && s.marks.contains(&TextMark::Bold)),
            "{spans:?}"
        );
        assert!(
            spans
                .iter()
                .any(|s| s.text.contains("link") && s.marks.contains(&TextMark::Link)),
            "{spans:?}"
        );
    }

    #[test]
    fn one_line_strips_markup() {
        assert_eq!(one_line("<b>Hello</b> world"), "Hello world");
    }

    #[test]
    fn empty_html_is_empty() {
        assert!(html_to_text("   ", 80).is_empty());
    }

    #[test]
    fn strip_tags_keeps_text() {
        assert_eq!(strip_tags("<p>Hello <em>there</em></p>"), "Hello there");
    }

    #[test]
    fn looks_empty_ignores_markup_only() {
        assert!(looks_empty(""));
        assert!(looks_empty("<p>  </p>"));
        assert!(!looks_empty("<p>A full sentence of story text.</p>"));
    }

    #[test]
    fn longest_html_prefers_more_text() {
        let short = "<p>Hi</p>";
        let long = "<p>A much longer description of the story.</p>";
        assert_eq!(longest_html(Some(short), Some(long)), Some(long));
        assert_eq!(longest_html(None, Some(short)), Some(short));
        assert_eq!(longest_html(None, None), None);
    }
}
