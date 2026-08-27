use crate::app::{App, Focus, Mode};
use crate::html::{RichSpan, TextMark, html_to_rich};
use crate::script::{self, contains_sinhala, ellipsize_shaped, render_shaped};
use crate::timefmt::{relative, wire_clock};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Clear, List, ListItem, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Wrap,
};

/// One Dark: slate background, cyan accent, no purple.
struct Theme {
    bg: Color,
    panel: Color,
    panel_alt: Color,
    select: Color,
    fg: Color,
    body: Color,
    dim: Color,
    cyan: Color,
    green: Color,
    yellow: Color,
    orange: Color,
    red: Color,
    blue: Color,
    bar_idle: Color,
}

impl Theme {
    fn one_dark() -> Self {
        Self {
            bg: Color::Rgb(40, 44, 52),
            panel: Color::Rgb(33, 37, 43),
            panel_alt: Color::Rgb(44, 49, 60),
            select: Color::Rgb(62, 68, 81),
            fg: Color::Rgb(220, 223, 228),
            body: Color::Rgb(171, 178, 191),
            dim: Color::Rgb(92, 99, 112),
            cyan: Color::Rgb(86, 182, 194),
            green: Color::Rgb(152, 195, 121),
            yellow: Color::Rgb(229, 192, 123),
            orange: Color::Rgb(209, 154, 102),
            red: Color::Rgb(224, 108, 117),
            blue: Color::Rgb(97, 175, 239),
            bar_idle: Color::Rgb(62, 68, 81),
        }
    }

    fn feed_swatch(&self, name: &str) -> Color {
        const SWATCHES: [fn(&Theme) -> Color; 6] = [
            |t| t.cyan,
            |t| t.blue,
            |t| t.green,
            |t| t.orange,
            |t| t.yellow,
            |t| t.red,
        ];
        let hash = name
            .bytes()
            .fold(0u8, |acc, b| acc.wrapping_add(b.wrapping_mul(31)));
        SWATCHES[(hash as usize) % SWATCHES.len()](self)
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let theme = Theme::one_dark();
    frame.render_widget(
        Block::new().style(Style::new().bg(theme.bg).fg(theme.fg)),
        frame.area(),
    );

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, header, app, &theme);

    let [upper, reader] =
        Layout::vertical([Constraint::Percentage(38), Constraint::Fill(1)]).areas(body);
    let [feeds, stories] =
        Layout::horizontal([Constraint::Length(30), Constraint::Fill(1)]).areas(upper);

    draw_feeds(frame, feeds, app, &theme);
    draw_stories(frame, stories, app, &theme);
    draw_reader(frame, reader, app, &theme);
    draw_footer(frame, footer, app, &theme);

    match app.mode {
        Mode::Help => draw_help(frame, &theme),
        Mode::AddFeed => draw_add_feed(frame, app, &theme),
        Mode::RemoveFeed => draw_remove_feed(frame, app, &theme),
        Mode::Search => {}
        Mode::ConfirmDelete => {
            let name = app
                .selected_remove_feed()
                .map(|f| f.title.as_str())
                .unwrap_or("this feed");
            draw_prompt(
                frame,
                "remove feed",
                &format!("Remove {name} and its stories? [y/n]"),
                "",
                theme.red,
                &theme,
            );
        }
        Mode::Normal => {}
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let [mast, ribbon] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

    let unread = app.total_unread();
    let clock = wire_clock(chrono::Utc::now());
    let mut mast_spans = vec![
        Span::styled(" ◆ ", Style::new().fg(theme.cyan).bold()),
        Span::styled("tui-news", Style::new().fg(theme.cyan).bold()),
        Span::styled(format!("  {clock}  "), Style::new().fg(theme.dim)),
    ];
    if unread > 0 {
        mast_spans.push(Span::styled(
            format!("{unread} unread"),
            Style::new().fg(theme.cyan).bold(),
        ));
    } else {
        mast_spans.push(Span::styled("caught up", Style::new().fg(theme.green)));
    }
    if app.fetching() {
        mast_spans.push(Span::styled(
            format!("  ● fetch {}", app.in_flight.len()),
            Style::new().fg(theme.yellow).bold(),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(mast_spans)).style(Style::new().bg(theme.bg)),
        mast,
    );

    let filter = if app.unread_only {
        Span::styled("unread only", Style::new().fg(theme.yellow).bold())
    } else {
        Span::styled("all stories", Style::new().fg(theme.dim))
    };
    let search = app.search.as_ref().map_or_else(
        || Span::raw(""),
        |q| Span::styled(format!("  /{q}"), Style::new().fg(theme.cyan)),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  feeds ", Style::new().fg(theme.dim)),
            Span::styled(
                app.feeds.len().to_string(),
                Style::new().fg(theme.blue).bold(),
            ),
            Span::styled("   stories ", Style::new().fg(theme.dim)),
            Span::styled(
                app.articles.len().to_string(),
                Style::new().fg(theme.cyan).bold(),
            ),
            Span::styled("   ", Style::new()),
            filter,
            search,
        ]))
        .style(Style::new().bg(theme.bg)),
        ribbon,
    );
}

fn draw_feeds(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let focused = app.focus == Focus::Feeds && app.mode == Mode::Normal;
    let content = accent_pane(frame, area, focused, theme.blue, theme);

    let mut items = Vec::with_capacity(app.feeds.len() + 1);
    items.push(feed_row(
        "All",
        app.total_unread(),
        None,
        theme.cyan,
        0,
        theme,
    ));
    for (i, feed) in app.feeds.iter().enumerate() {
        items.push(feed_row(
            &feed.title,
            feed.unread,
            feed.error.as_deref(),
            theme.feed_swatch(&feed.title),
            i + 1,
            theme,
        ));
    }

    let block = pane_block("feeds", focused, theme.blue, theme);
    let inner = block.inner(content);
    frame.render_widget(block, content);
    let selected = app.feed_state.selected().unwrap_or(0);
    let offset = visible_offset(items.len(), selected, inner.height as usize);
    for (row, item) in items.iter().skip(offset).take(inner.height as usize).enumerate() {
        let y = inner.y + row as u16;
        let area = Rect::new(inner.x, y, inner.width, 1);
        let is_sel = offset + row == selected;
        let bg = if is_sel {
            theme.select
        } else {
            item.row_bg
        };
        frame.buffer_mut().set_style(area, Style::new().bg(bg));
        let prefix = if is_sel { "▸ " } else { "  " };
        let prefix_w: u16 = 4;
        render_shaped(
            frame.buffer_mut(),
            Rect::new(area.x, y, 2, 1),
            prefix,
            Style::new().fg(if is_sel { theme.fg } else { theme.dim }).bg(bg),
        );
        render_shaped(
            frame.buffer_mut(),
            Rect::new(area.x + 2, y, 2, 1),
            item.mark,
            Style::new().fg(item.mark_color).bg(bg).bold(),
        );
        let title_w = area.width.saturating_sub(prefix_w + 4); // count column
        render_shaped(
            frame.buffer_mut(),
            Rect::new(area.x + prefix_w, y, title_w, 1),
            &ellipsize_shaped(&item.title, title_w as usize),
            Style::new().fg(theme.fg).bg(bg),
        );
        render_shaped(
            frame.buffer_mut(),
            Rect::new(area.right().saturating_sub(4), y, 4, 1),
            &item.count,
            Style::new().fg(item.mark_color).bg(bg).bold(),
        );
    }
}

struct RowData {
    title: String,
    mark: &'static str,
    mark_color: Color,
    count: String,
    row_bg: Color,
}

fn feed_row(
    title: &str,
    unread: i64,
    error: Option<&str>,
    accent: Color,
    row: usize,
    theme: &Theme,
) -> RowData {
    let row_bg = if row % 2 == 1 {
        theme.panel_alt
    } else {
        theme.panel
    };
    let (mark, mark_color) = if error.is_some() {
        ("● ", theme.red)
    } else if unread > 0 {
        ("● ", accent)
    } else {
        ("○ ", theme.dim)
    };
    let count = if unread > 0 {
        format!("{unread:>3}")
    } else {
        "  ·".into()
    };
    RowData {
        title: title.to_string(),
        mark,
        mark_color,
        count,
        row_bg,
    }
}

fn visible_offset(len: usize, selected: usize, height: usize) -> usize {
    if height == 0 || len == 0 {
        return 0;
    }
    let selected = selected.min(len.saturating_sub(1));
    if selected < height {
        0
    } else {
        selected + 1 - height
    }
}

fn draw_stories(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let focused = app.focus == Focus::Articles && app.mode == Mode::Normal;
    let content = accent_pane(frame, area, focused, theme.cyan, theme);
    let now = chrono::Utc::now().timestamp();

    let mut label = String::from("stories");
    if app.unread_only {
        label.push_str(" · unread");
    }
    if let Some(q) = &app.search {
        label.push_str(" · /");
        label.push_str(q);
    }

    let block = pane_block(&label, focused, theme.cyan, theme);
    let inner = block.inner(content);
    frame.render_widget(block, content);

    if app.articles.is_empty() {
        render_shaped(
            frame.buffer_mut(),
            inner,
            "no stories — press r to refresh",
            Style::new().fg(theme.cyan),
        );
        return;
    }

    let selected = app.article_state.selected().unwrap_or(0);
    let offset = visible_offset(app.articles.len(), selected, inner.height as usize);
    let suffix_w = 10u16;
    let title_w = inner.width.saturating_sub(4 + suffix_w);

    for (row, article) in app
        .articles
        .iter()
        .skip(offset)
        .take(inner.height as usize)
        .enumerate()
    {
        let y = inner.y + row as u16;
        let is_sel = offset + row == selected;
        let bg = if is_sel {
            theme.select
        } else if (offset + row) % 2 == 1 {
            theme.panel_alt
        } else {
            theme.panel
        };
        let row_area = Rect::new(inner.x, y, inner.width, 1);
        frame.buffer_mut().set_style(row_area, Style::new().bg(bg));
        let accent = theme.feed_swatch(&article.feed_title);
        let mark = if article.is_read { "○ " } else { "● " };
        let mark_color = if article.is_read {
            theme.dim
        } else {
            theme.cyan
        };
        let title_style = if article.is_read {
            Style::new().fg(theme.dim).bg(bg)
        } else {
            Style::new().fg(theme.fg).bg(bg).bold()
        };
        let when = article
            .published
            .map(|ts| relative(ts, now))
            .unwrap_or_else(|| "—".into());
        render_shaped(
            frame.buffer_mut(),
            Rect::new(inner.x, y, 2, 1),
            mark,
            Style::new().fg(mark_color).bg(bg).bold(),
        );
        render_shaped(
            frame.buffer_mut(),
            Rect::new(inner.x + 2, y, title_w, 1),
            &ellipsize_shaped(&article.title, title_w as usize),
            title_style,
        );
        let suffix = format!("{when:>4} {}", feed_tag(&article.feed_title));
        render_shaped(
            frame.buffer_mut(),
            Rect::new(inner.right().saturating_sub(suffix_w), y, suffix_w, 1),
            &suffix,
            Style::new().fg(accent).bg(bg),
        );
    }
}

fn draw_reader(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let focused = app.focus == Focus::Reader && app.mode == Mode::Normal;
    let content = accent_pane(frame, area, focused, theme.cyan, theme);
    let block = pane_block("story", focused, theme.cyan, theme);
    let inner = block.inner(content);
    frame.render_widget(block, content);

    let padded = Rect {
        x: inner.x.saturating_add(1),
        y: inner.y,
        width: inner.width.saturating_sub(2),
        height: inner.height,
    };

    let Some(article) = app.selected_article().cloned() else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Choose a story from the list above.",
                    Style::new().fg(theme.fg),
                )),
                Line::from(Span::styled(
                    "Press n for the next unread, or o to open in a browser.",
                    Style::new().fg(theme.dim),
                )),
            ])
            .alignment(Alignment::Left),
            padded,
        );
        return;
    };

    let now = chrono::Utc::now().timestamp();
    let when = article
        .published
        .map(|ts| relative(ts, now))
        .unwrap_or_else(|| "unknown".into());
    let accent = theme.feed_swatch(&article.feed_title);
    let title_lines = title_height(&article.title, padded.width);

    let [kicker_area, title_area, byline_area, rule_area, body_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(title_lines),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .areas(padded);

    if contains_sinhala(&article.feed_title) {
        let kicker = format!("{}  ·  {}", article.feed_title, when);
        render_shaped(
            frame.buffer_mut(),
            kicker_area,
            &kicker,
            Style::new().fg(accent).bold(),
        );
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    article.feed_title.to_uppercase(),
                    Style::new().fg(accent).bold(),
                ),
                Span::styled("  ·  ", Style::new().fg(theme.dim)),
                Span::styled(when, Style::new().fg(theme.dim)),
            ])),
            kicker_area,
        );
    }

    render_shaped(
        frame.buffer_mut(),
        title_area,
        &article.title,
        Style::new().fg(theme.fg).bold(),
    );

    let mut byline = vec![
        if article.is_read {
            Span::styled("read", Style::new().fg(theme.green))
        } else {
            Span::styled("unread", Style::new().fg(theme.orange).bold())
        },
        Span::styled("   ", Style::new()),
        Span::styled("o", Style::new().fg(theme.cyan).bold()),
        Span::styled(" open original", Style::new().fg(theme.dim)),
        Span::styled("   j/k", Style::new().fg(theme.cyan).bold()),
        Span::styled(" scroll", Style::new().fg(theme.dim)),
    ];
    if let Some(url) = article.url.as_deref() {
        byline.push(Span::styled("   ", Style::new()));
        byline.push(Span::styled(
            ellipsize_shaped(url, padded.width.saturating_sub(36) as usize),
            Style::new().fg(theme.dim),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(byline)), byline_area);

    let rule = "─".repeat(padded.width as usize);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            rule,
            Style::new().fg(theme.bar_idle),
        ))),
        rule_area,
    );

    if !article.has_body() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "This feed only sent a headline — no article text.",
                    Style::new().fg(theme.yellow),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press o to read it in the browser.",
                    Style::new().fg(theme.body),
                )),
            ])
            .wrap(Wrap { trim: true }),
            body_area,
        );
        return;
    }

    let wrap_width = body_area.width.max(24) as usize;
    let (body_text, line_count) = shaped_body(article.body_html(), wrap_width);
    let max_scroll = line_count.saturating_sub(body_area.height);
    app.clamp_reader_scroll(max_scroll);

    if contains_sinhala(&body_text) {
        let start = app.reader_scroll as usize;
        let visible: String = script::wrap_shaped(&body_text, wrap_width)
            .into_iter()
            .skip(start)
            .take(body_area.height as usize)
            .collect::<Vec<_>>()
            .join("\n");
        render_shaped(
            frame.buffer_mut(),
            body_area,
            &visible,
            Style::new().fg(theme.body),
        );
    } else {
        let rich = html_to_rich(article.body_html(), wrap_width);
        let lines: Vec<Line> = rich.iter().map(|spans| rich_line(spans, theme)).collect();
        frame.render_widget(
            Paragraph::new(lines)
                .style(Style::new().fg(theme.body))
                .scroll((app.reader_scroll, 0)),
            body_area,
        );
    }

    if line_count > body_area.height {
        let mut state =
            ScrollbarState::new(line_count as usize).position(app.reader_scroll as usize);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::new().fg(theme.bar_idle))
                .thumb_style(Style::new().fg(theme.cyan)),
            body_area,
            &mut state,
        );
    }
}

fn title_height(title: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    let cols = script::display_width(title).max(1) as usize;
    let lines = cols.div_ceil(width) as u16;
    lines.clamp(2, 4)
}

fn shaped_body(html: &str, width: usize) -> (String, u16) {
    let rich = html_to_rich(html, 10_000);
    let mut body = String::new();
    for line in &rich {
        if line.iter().all(|span| span.text.trim().is_empty()) {
            if !body.ends_with('\n') {
                body.push('\n');
            }
            body.push('\n');
            continue;
        }
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        for span in line {
            body.push_str(&span.text);
        }
    }
    let line_count = script::wrap_shaped(&body, width).len() as u16;
    (body, line_count)
}

fn rich_line(spans: &[RichSpan], theme: &Theme) -> Line<'static> {
    if spans.is_empty() || spans.iter().all(|s| s.text.trim().is_empty()) {
        return Line::from("");
    }
    let heading = spans
        .iter()
        .all(|s| s.text.trim().is_empty() || s.marks.contains(&TextMark::Bold));
    Line::from(
        spans
            .iter()
            .map(|span| Span::styled(span.text.clone(), mark_style(&span.marks, heading, theme)))
            .collect::<Vec<_>>(),
    )
}

fn mark_style(marks: &[TextMark], heading: bool, theme: &Theme) -> Style {
    if heading {
        return Style::new().fg(theme.fg).bold();
    }
    let mut style = Style::new().fg(theme.body);
    for mark in marks {
        match mark {
            TextMark::Italic => {
                style = style.fg(theme.yellow).add_modifier(Modifier::ITALIC);
            }
            TextMark::Code => {
                style = style.fg(theme.green);
            }
            TextMark::Strike => {
                style = style.fg(theme.dim).add_modifier(Modifier::CROSSED_OUT);
            }
            TextMark::Bold => {
                style = style.fg(theme.fg).add_modifier(Modifier::BOLD);
            }
            TextMark::Link => {
                style = style.fg(theme.cyan).add_modifier(Modifier::UNDERLINED);
            }
        }
    }
    style
}

fn draw_add_feed(frame: &mut Frame, app: &mut App, theme: &Theme) {
    let area = centered(frame.area(), 74, 18);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(Span::styled(
            " add feed ",
            Style::new().fg(theme.cyan).bold(),
        ))
        .border_style(Style::new().fg(theme.cyan))
        .style(Style::new().bg(theme.panel).fg(theme.fg))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [url_area, hint_area, list_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" › ", Style::new().fg(theme.cyan).bold()),
            Span::styled(app.input.clone(), Style::new().fg(theme.fg).bold()),
            Span::styled("█", Style::new().fg(theme.cyan)),
        ])),
        url_area,
    );
    frame.render_widget(
        Paragraph::new("type to search popular feeds  ·  paste any RSS/Atom URL  ·  ↑↓ then enter")
            .style(Style::new().fg(theme.dim)),
        hint_area,
    );

    let matches = app.catalog_matches();
    let items: Vec<ListItem> = if matches.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "no matching feeds — paste a full RSS URL instead",
            Style::new().fg(theme.yellow).italic(),
        )))]
    } else {
        matches
            .iter()
            .map(|feed| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<9}", feed.kind), Style::new().fg(theme.orange)),
                    Span::styled(feed.name, Style::new().fg(theme.fg).bold()),
                    Span::styled(format!("  {}", feed.url), Style::new().fg(theme.dim)),
                ]))
            })
            .collect()
    };

    let list = List::new(items)
        .highlight_style(
            Style::new()
                .bg(theme.select)
                .fg(theme.fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, list_area, &mut app.catalog_state);
}

fn draw_remove_feed(frame: &mut Frame, app: &mut App, theme: &Theme) {
    let area = centered(frame.area(), 74, 16);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(Span::styled(
            " remove feed ",
            Style::new().fg(theme.red).bold(),
        ))
        .border_style(Style::new().fg(theme.red))
        .style(Style::new().bg(theme.panel).fg(theme.fg))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [hint_area, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);

    frame.render_widget(
        Paragraph::new("↑↓ pick a feed   enter confirm   esc cancel")
            .style(Style::new().fg(theme.dim)),
        hint_area,
    );

    let items: Vec<ListItem> = if app.feeds.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "no feeds subscribed",
            Style::new().fg(theme.yellow).italic(),
        )))]
    } else {
        app.feeds
            .iter()
            .map(|feed| {
                let unread = if feed.unread > 0 {
                    Span::styled(
                        format!("  {} unread", feed.unread),
                        Style::new().fg(theme.cyan),
                    )
                } else {
                    Span::raw("")
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:<22}", ellipsize_shaped(&feed.title, 22)),
                        Style::new().fg(theme.fg).bold(),
                    ),
                    unread,
                    Span::styled(format!("  {}", feed.url), Style::new().fg(theme.dim)),
                ]))
            })
            .collect()
    };

    let list = List::new(items)
        .highlight_style(
            Style::new()
                .bg(theme.select)
                .fg(theme.fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, list_area, &mut app.remove_state);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let content = if app.mode == Mode::Search {
        Line::from(vec![
            Span::styled(" /", Style::new().fg(theme.cyan).bold()),
            Span::styled(format!(" {}█", app.input), Style::new().fg(theme.fg)),
            Span::styled("   enter apply   esc clear", Style::new().fg(theme.dim)),
        ])
    } else if app.mode == Mode::AddFeed {
        Line::from(vec![
            cmd("↑↓", theme),
            hint("pick", theme),
            cmd("enter", theme),
            hint("add", theme),
            cmd("tab", theme),
            hint("use url", theme),
            cmd("esc", theme),
            hint("cancel", theme),
        ])
    } else if app.mode == Mode::RemoveFeed {
        Line::from(vec![
            cmd("↑↓", theme),
            hint("pick", theme),
            cmd("enter", theme),
            hint("remove", theme),
            cmd("esc", theme),
            hint("cancel", theme),
        ])
    } else if app.mode == Mode::ConfirmDelete {
        Line::from(vec![
            cmd("y", theme),
            hint("remove", theme),
            cmd("n", theme),
            hint("back", theme),
            cmd("esc", theme),
            hint("back", theme),
        ])
    } else if !app.status.is_empty() {
        let status_style = if app.status_error {
            Style::new().fg(theme.red).bold()
        } else {
            Style::new().fg(theme.green)
        };
        Line::from(vec![
            Span::raw("  "),
            Span::styled(app.status.clone(), status_style),
            Span::raw("   "),
            cmd("?", theme),
            hint("keys", theme),
            cmd("q", theme),
            hint("quit", theme),
        ])
    } else {
        Line::from(vec![
            cmd("j/k", theme),
            hint("move", theme),
            cmd("tab", theme),
            hint("pane", theme),
            cmd("r", theme),
            hint("refresh", theme),
            cmd("o", theme),
            hint("open", theme),
            cmd("n", theme),
            hint("unread", theme),
            cmd("a", theme),
            hint("add", theme),
            cmd("d", theme),
            hint("remove", theme),
            cmd("/", theme),
            hint("find", theme),
            cmd("?", theme),
            hint("help", theme),
        ])
    };
    frame.render_widget(
        Paragraph::new(content).style(Style::new().bg(theme.bg)),
        area,
    );
}

fn draw_help(frame: &mut Frame, theme: &Theme) {
    let area = centered(frame.area(), 66, 22);
    frame.render_widget(Clear, area);
    let text = vec![
        Line::from(Span::styled(
            "◆ movement",
            Style::new().fg(theme.blue).bold(),
        )),
        Line::from(vec![
            Span::styled("  j k ↑ ↓     ", Style::new().fg(theme.cyan).bold()),
            Span::raw("move in the focused pane"),
        ]),
        Line::from(vec![
            Span::styled("  tab h l     ", Style::new().fg(theme.cyan).bold()),
            Span::raw("cycle feeds / stories / reader"),
        ]),
        Line::from(vec![
            Span::styled("  g G         ", Style::new().fg(theme.cyan).bold()),
            Span::raw("top / bottom"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "◆ stories",
            Style::new().fg(theme.cyan).bold(),
        )),
        Line::from(vec![
            Span::styled("  enter o     ", Style::new().fg(theme.cyan).bold()),
            Span::raw("open in browser (marks read)"),
        ]),
        Line::from(vec![
            Span::styled("  space m     ", Style::new().fg(theme.cyan).bold()),
            Span::raw("toggle read"),
        ]),
        Line::from(vec![
            Span::styled("  n p         ", Style::new().fg(theme.cyan).bold()),
            Span::raw("next / previous unread"),
        ]),
        Line::from(vec![
            Span::styled("  u / A       ", Style::new().fg(theme.cyan).bold()),
            Span::raw("unread-only filter / mark visible read"),
        ]),
        Line::from(""),
        Line::from(Span::styled("◆ feeds", Style::new().fg(theme.green).bold())),
        Line::from(vec![
            Span::styled("  r R         ", Style::new().fg(theme.green).bold()),
            Span::raw("refresh selected / all"),
        ]),
        Line::from(vec![
            Span::styled("  a           ", Style::new().fg(theme.green).bold()),
            Span::raw("add a feed (catalog or URL)"),
        ]),
        Line::from(vec![
            Span::styled("  d Del       ", Style::new().fg(theme.green).bold()),
            Span::raw("remove a feed and its stories"),
        ]),
        Line::from(vec![
            Span::styled("  q           ", Style::new().fg(theme.green).bold()),
            Span::raw("quit"),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(text).block(
            Block::bordered()
                .title(Span::styled(" keys ", Style::new().fg(theme.cyan).bold()))
                .border_style(Style::new().fg(theme.cyan))
                .style(Style::new().bg(theme.panel).fg(theme.fg))
                .padding(Padding::uniform(1)),
        ),
        area,
    );
}

fn draw_prompt(
    frame: &mut Frame,
    title: &str,
    hint: &str,
    input: &str,
    accent: Color,
    theme: &Theme,
) {
    let area = centered(frame.area(), 64, 8);
    frame.render_widget(Clear, area);
    let body = vec![
        Line::from(Span::styled(hint, Style::new().fg(theme.dim))),
        Line::from(""),
        Line::from(vec![
            Span::styled(" › ", Style::new().fg(accent).bold()),
            Span::styled(input.to_string(), Style::new().fg(theme.fg).bold()),
            Span::styled("█", Style::new().fg(accent)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(body).block(
            Block::bordered()
                .title(Span::styled(
                    format!(" {title} "),
                    Style::new().fg(accent).bold(),
                ))
                .border_style(Style::new().fg(accent))
                .style(Style::new().bg(theme.panel).fg(theme.fg))
                .padding(Padding::horizontal(1)),
        ),
        area,
    );
}

fn accent_pane(frame: &mut Frame, area: Rect, focused: bool, accent: Color, theme: &Theme) -> Rect {
    let [bar, rest] = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    frame.render_widget(
        Block::new().style(Style::new().bg(if focused { accent } else { theme.bar_idle })),
        bar,
    );
    rest
}

fn pane_block<'a>(title: &'a str, focused: bool, accent: Color, theme: &'a Theme) -> Block<'a> {
    let title_style = if focused {
        Style::new().fg(accent).bold()
    } else {
        Style::new().fg(theme.dim)
    };
    Block::new()
        .title(Span::styled(format!(" {title} "), title_style))
        .style(Style::new().bg(theme.panel).fg(theme.fg))
}

fn cmd<'a>(label: &'a str, theme: &'a Theme) -> Span<'a> {
    Span::styled(format!(" {label}"), Style::new().fg(theme.cyan).bold())
}

fn hint<'a>(text: &'a str, theme: &'a Theme) -> Span<'a> {
    Span::styled(format!(" {text}  "), Style::new().fg(theme.dim))
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    )
}

fn feed_tag(name: &str) -> String {
    let initials: String = name
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .collect();
    if initials.chars().count() >= 2 {
        initials.to_uppercase()
    } else {
        name.chars().take(3).collect::<String>().to_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use super::feed_tag;
    use crate::script::ellipsize_shaped;

    #[test]
    fn ellipsize_short_is_unchanged() {
        assert_eq!(ellipsize_shaped("hello", 10), "hello");
    }

    #[test]
    fn ellipsize_long_adds_ellipsis() {
        assert_eq!(ellipsize_shaped("hello world", 8), "hello w…");
    }

    #[test]
    fn ellipsize_keeps_sinhala_clusters() {
        let title = "ශ්‍රී ලංකාවේ පුවත්";
        let cut = ellipsize_shaped(title, 8);
        assert!(cut.ends_with('…'), "{cut}");
        assert!(
            !cut.trim_end_matches('…').ends_with('\u{0DCA}'),
            "dangling virama: {cut}"
        );
    }

    #[test]
    fn feed_tag_uses_initials() {
        assert_eq!(feed_tag("Hacker News"), "HN");
        assert_eq!(feed_tag("BBC World"), "BW");
        assert_eq!(feed_tag("Lobsters"), "LOB");
    }
}
