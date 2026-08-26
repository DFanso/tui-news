use crate::app::{App, Focus, Mode};
use crate::html::html_to_text;
use crate::timefmt::{relative, wire_clock};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Wrap,
};

struct Theme {
    bg: Color,
    fg: Color,
    dim: Color,
    ink: Color,
    unread: Color,
    border: Color,
    error: Color,
    selected_fg: Color,
}

impl Theme {
    fn wire() -> Self {
        Self {
            bg: Color::Rgb(16, 15, 12),
            fg: Color::Rgb(226, 216, 196),
            dim: Color::Rgb(118, 110, 92),
            ink: Color::Rgb(10, 9, 7),
            unread: Color::Rgb(232, 168, 42),
            border: Color::Rgb(74, 66, 48),
            error: Color::Rgb(196, 78, 48),
            selected_fg: Color::Rgb(16, 15, 12),
        }
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let theme = Theme::wire();
    frame.render_widget(
        Block::new().style(Style::new().bg(theme.bg).fg(theme.fg)),
        frame.area(),
    );

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, header, app, &theme);

    let [upper, reader] =
        Layout::vertical([Constraint::Percentage(48), Constraint::Fill(1)]).areas(body);
    let [feeds, stories] =
        Layout::horizontal([Constraint::Length(28), Constraint::Fill(1)]).areas(upper);

    draw_feeds(frame, feeds, app, &theme);
    draw_stories(frame, stories, app, &theme);
    draw_reader(frame, reader, app, &theme);
    draw_footer(frame, footer, app, &theme);

    match app.mode {
        Mode::Help => draw_help(frame, &theme),
        Mode::AddFeed => draw_prompt(
            frame,
            " add feed ",
            "Paste an RSS or Atom URL",
            &app.input,
            &theme,
        ),
        Mode::Search => {}
        Mode::ConfirmDelete => {
            let name = app
                .selected_feed()
                .map(|f| f.title.as_str())
                .unwrap_or("this feed");
            draw_prompt(
                frame,
                " delete feed ",
                &format!("Remove {name} and its stories? [y/n]"),
                "",
                &theme,
            );
        }
        Mode::Normal => {}
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let unread = app.total_unread();
    let clock = wire_clock(chrono::Utc::now());
    let fetch = if app.fetching() {
        format!("  FETCH {}", app.in_flight.len())
    } else {
        String::new()
    };
    let line = Line::from(vec![
        Span::styled(
            " TUI-NEWS ",
            Style::new().fg(theme.ink).bg(theme.unread).bold(),
        ),
        Span::styled(format!("  {clock}  "), Style::new().fg(theme.dim)),
        Span::styled(
            format!("{unread} UNREAD"),
            Style::new()
                .fg(if unread > 0 { theme.unread } else { theme.dim })
                .bold(),
        ),
        Span::styled(fetch, Style::new().fg(theme.unread)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_feeds(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let focused = app.focus == Focus::Feeds && app.mode == Mode::Normal;
    let mut items = Vec::with_capacity(app.feeds.len() + 1);
    items.push(feed_item("All", app.total_unread(), None, theme));
    for feed in &app.feeds {
        items.push(feed_item(
            &feed.title,
            feed.unread,
            feed.error.as_deref(),
            theme,
        ));
    }

    let list = List::new(items)
        .block(pane("FEEDS", focused, theme))
        .highlight_style(
            Style::new()
                .bg(theme.unread)
                .fg(theme.selected_fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, area, &mut app.feed_state);
}

fn feed_item<'a>(
    title: &'a str,
    unread: i64,
    error: Option<&str>,
    theme: &'a Theme,
) -> ListItem<'a> {
    let mark = if error.is_some() {
        Span::styled("! ", Style::new().fg(theme.error).bold())
    } else if unread > 0 {
        Span::styled("● ", Style::new().fg(theme.unread))
    } else {
        Span::styled("  ", Style::new())
    };
    let count = if unread > 0 {
        Span::styled(format!(" {unread}"), Style::new().fg(theme.unread))
    } else {
        Span::styled(" 0", Style::new().fg(theme.dim))
    };
    ListItem::new(Line::from(vec![
        mark,
        Span::raw(ellipsize(title, 16)),
        count,
    ]))
}

fn draw_stories(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let focused = app.focus == Focus::Articles && app.mode == Mode::Normal;
    let now = chrono::Utc::now().timestamp();
    let inner_width = area.width.saturating_sub(4) as usize;
    let title_width = inner_width.saturating_sub(12).max(8);

    let items: Vec<ListItem> = if app.articles.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "no stories — press r to refresh",
            Style::new().fg(theme.dim).italic(),
        )))]
    } else {
        app.articles
            .iter()
            .map(|article| {
                let bullet = if article.is_read {
                    Span::styled("○ ", Style::new().fg(theme.dim))
                } else {
                    Span::styled("● ", Style::new().fg(theme.unread).bold())
                };
                let title_style = if article.is_read {
                    Style::new().fg(theme.dim)
                } else {
                    Style::new().fg(theme.fg).bold()
                };
                let when = article
                    .published
                    .map(|ts| relative(ts, now))
                    .unwrap_or_else(|| "—".into());
                ListItem::new(Line::from(vec![
                    bullet,
                    Span::styled(ellipsize(&article.title, title_width), title_style),
                    Span::styled(format!(" {when:>4}"), Style::new().fg(theme.dim)),
                ]))
            })
            .collect()
    };

    let mut title = String::from("STORIES");
    if app.unread_only {
        title.push_str(" · unread");
    }
    if let Some(q) = &app.search {
        title.push_str(" · /");
        title.push_str(q);
    }

    let list = List::new(items)
        .block(pane(&title, focused, theme))
        .highlight_style(
            Style::new()
                .bg(theme.unread)
                .fg(theme.selected_fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");
    frame.render_stateful_widget(list, area, &mut app.article_state);
}

fn draw_reader(frame: &mut Frame, area: Rect, app: &mut App, theme: &Theme) {
    let focused = app.focus == Focus::Reader && app.mode == Mode::Normal;
    let block = pane("STORY", focused, theme).padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(article) = app.selected_article().cloned() else {
        frame.render_widget(
            Paragraph::new("Select a story to read.")
                .style(Style::new().fg(theme.dim).italic())
                .alignment(Alignment::Left),
            inner,
        );
        return;
    };

    let now = chrono::Utc::now().timestamp();
    let when = article
        .published
        .map(|ts| relative(ts, now))
        .unwrap_or_else(|| "unknown time".into());
    let state = if article.is_read { "read" } else { "unread" };
    let meta = format!("{}  ·  {when}  ·  {state}", article.feed_title);

    let [title_area, meta_area, body_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(inner);

    frame.render_widget(
        Paragraph::new(article.title.clone())
            .style(
                Style::new()
                    .fg(if article.is_read {
                        theme.fg
                    } else {
                        theme.unread
                    })
                    .bold(),
            )
            .wrap(Wrap { trim: true }),
        title_area,
    );
    frame.render_widget(
        Paragraph::new(meta).style(Style::new().fg(theme.dim)),
        meta_area,
    );

    let width = body_area.width.saturating_sub(1) as usize;
    let body = html_to_text(article.body_html(), width.max(20));
    let line_count = body.lines().count() as u16;
    let view_h = body_area.height;
    let max_scroll = line_count.saturating_sub(view_h);
    app.clamp_reader_scroll(max_scroll);

    frame.render_widget(
        Paragraph::new(body)
            .style(Style::new().fg(theme.fg))
            .wrap(Wrap { trim: false })
            .scroll((app.reader_scroll, 0)),
        body_area,
    );

    if line_count > view_h {
        let mut state =
            ScrollbarState::new(line_count as usize).position(app.reader_scroll as usize);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::new().fg(theme.border))
                .thumb_style(Style::new().fg(theme.unread)),
            body_area,
            &mut state,
        );
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let content = if app.mode == Mode::Search {
        Line::from(vec![
            Span::styled(" /", Style::new().fg(theme.unread).bold()),
            Span::raw(app.input.clone()),
            Span::styled("█", Style::new().fg(theme.unread)),
            Span::styled("  enter apply  esc clear", Style::new().fg(theme.dim)),
        ])
    } else if !app.status.is_empty() {
        let style = if app.status_error {
            Style::new().fg(theme.error)
        } else {
            Style::new().fg(theme.unread)
        };
        Line::from(vec![
            Span::styled(" ", Style::new()),
            Span::styled(app.status.clone(), style),
            Span::styled(
                "    q quit  ? keys  r refresh  a add  / find",
                Style::new().fg(theme.dim),
            ),
        ])
    } else {
        Line::from(Span::styled(
            " j/k move  tab pane  r refresh  o open  space read  n next unread  a add  / find  ? help  q quit",
            Style::new().fg(theme.dim),
        ))
    };
    frame.render_widget(Paragraph::new(content), area);
}

fn draw_help(frame: &mut Frame, theme: &Theme) {
    let area = centered(frame.area(), 64, 18);
    frame.render_widget(Clear, area);
    let text = vec![
        Line::from(Span::styled(
            "movement",
            Style::new().fg(theme.unread).bold(),
        )),
        Line::from("  j k ↑ ↓     move in the focused pane"),
        Line::from("  tab h l     cycle feeds / stories / reader"),
        Line::from("  g G         top / bottom"),
        Line::from(""),
        Line::from(Span::styled(
            "stories",
            Style::new().fg(theme.unread).bold(),
        )),
        Line::from("  enter o     open in browser (marks read)"),
        Line::from("  space m     toggle read"),
        Line::from("  n p         next / previous unread"),
        Line::from("  u           unread-only filter    A  mark visible read"),
        Line::from("  /           search titles"),
        Line::from(""),
        Line::from(Span::styled("feeds", Style::new().fg(theme.unread).bold())),
        Line::from("  r R         refresh selected / all"),
        Line::from("  a           add feed URL          d  delete feed"),
        Line::from("  q           quit"),
    ];
    frame.render_widget(
        Paragraph::new(text).block(
            Block::bordered()
                .title(" keys ")
                .border_style(Style::new().fg(theme.unread))
                .style(Style::new().bg(theme.bg).fg(theme.fg))
                .padding(Padding::uniform(1)),
        ),
        area,
    );
}

fn draw_prompt(frame: &mut Frame, title: &str, hint: &str, input: &str, theme: &Theme) {
    let area = centered(frame.area(), 62, 7);
    frame.render_widget(Clear, area);
    let body = vec![
        Line::from(Span::styled(hint, Style::new().fg(theme.dim))),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::new().fg(theme.unread)),
            Span::raw(input),
            Span::styled("█", Style::new().fg(theme.unread)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(body).block(
            Block::bordered()
                .title(title)
                .border_style(Style::new().fg(theme.unread))
                .style(Style::new().bg(theme.bg).fg(theme.fg))
                .padding(Padding::horizontal(1)),
        ),
        area,
    );
}

fn pane<'a>(title: &'a str, focused: bool, theme: &'a Theme) -> Block<'a> {
    Block::bordered()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::new().fg(if focused { theme.unread } else { theme.border }))
        .style(Style::new().bg(theme.bg).fg(theme.fg))
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

fn ellipsize(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let take = max_chars.saturating_sub(1);
    let mut out: String = s.chars().take(take).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::ellipsize;

    #[test]
    fn ellipsize_short_is_unchanged() {
        assert_eq!(ellipsize("hello", 10), "hello");
    }

    #[test]
    fn ellipsize_long_adds_ellipsis() {
        assert_eq!(ellipsize("hello world", 8), "hello w…");
    }
}
