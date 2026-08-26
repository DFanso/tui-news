/// Convert RSS/Atom HTML (or plain text) into wrapped terminal text.
pub fn html_to_text(html: &str, width: usize) -> String {
    let width = width.max(20);
    let trimmed = html.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    html2text::config::plain()
        .allow_width_overflow()
        .string_from_read(trimmed.as_bytes(), width)
        .unwrap_or_else(|_| strip_tags(trimmed))
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
}
