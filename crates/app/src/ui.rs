//! Small styling helpers shared by the views.

use iced::font::{Font, Weight};
use iced::widget::text::{Rich, Span};
use iced::widget::{Text, button, container, rich_text, span, text};
use iced::{Color, Element, Theme};

use crate::search;

pub fn bold<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    text(s).font(Font {
        weight: Weight::Bold,
        ..Font::DEFAULT
    })
}

pub fn heading<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    bold(s).size(20)
}

fn muted_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.extended_palette().background.strongest.color),
    }
}

pub fn muted<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    text(s).size(13).style(muted_style)
}

/// Translucent yellow, readable on both light and dark themes.
const HIGHLIGHT: Color = Color {
    r: 1.0,
    g: 0.78,
    b: 0.0,
    a: 0.45,
};

/// `s` as spans, with every search match highlighted.
pub fn highlight<'a>(s: &str, needle: &str) -> Vec<Span<'a, (), Font>> {
    let mut spans = Vec::new();
    let mut last = 0;
    for m in search::find(s, needle) {
        if m.start > last {
            spans.push(span(s[last..m.start].to_string()));
        }
        spans.push(span(s[m.clone()].to_string()).background(HIGHLIGHT));
        last = m.end;
    }
    if last < s.len() || spans.is_empty() {
        spans.push(span(s[last..].to_string()));
    }
    spans
}

/// Several fields on one line, separated by " · ", each highlighted.
pub fn highlight_joined<'a>(parts: &[&str], needle: &str) -> Vec<Span<'a, (), Font>> {
    let mut spans = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            spans.push(span(" · "));
        }
        spans.extend(highlight(part, needle));
    }
    spans
}

pub fn rich<'a, M: 'a>(spans: Vec<Span<'a, (), Font>>) -> Rich<'a, (), M> {
    rich_text(spans)
}

pub fn rich_bold<'a, M: 'a>(spans: Vec<Span<'a, (), Font>>) -> Rich<'a, (), M> {
    rich_text(spans).font(Font {
        weight: Weight::Bold,
        ..Font::DEFAULT
    })
}

pub fn rich_muted<'a, M: 'a>(spans: Vec<Span<'a, (), Font>>) -> Rich<'a, (), M> {
    rich_text(spans).size(13).style(muted_style)
}

/// A clickable tag chip; `selected` when it's part of the current search.
pub fn tag<'a, M: Clone + 'a>(label: &str, selected: bool, on_press: M) -> Element<'a, M> {
    button(text(label.to_string()).size(12))
        .padding([2, 8])
        .style(move |theme: &Theme, status| {
            let base = if selected {
                button::primary(theme, status)
            } else {
                button::secondary(theme, status)
            };
            button::Style {
                border: iced::border::rounded(10),
                ..base
            }
        })
        .on_press(on_press)
        .into()
}

pub fn card<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(12)
        .width(iced::Fill)
        .style(container::bordered_box)
        .into()
}

pub fn chip<'a, M: 'a>(label: &'a str) -> Element<'a, M> {
    container(text(label).size(12))
        .padding([2, 8])
        .style(|theme: &Theme| {
            let pair = theme.extended_palette().primary.weak;
            container::Style {
                background: Some(pair.color.into()),
                text_color: Some(pair.text),
                border: iced::border::rounded(10),
                ..container::Style::default()
            }
        })
        .into()
}

pub fn banner<'a, M: 'a>(message: &'a str, is_error: bool) -> Element<'a, M> {
    container(text(message).size(14))
        .padding(8)
        .width(iced::Fill)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let pair = if is_error {
                palette.danger.weak
            } else {
                palette.success.weak
            };
            container::Style {
                background: Some(pair.color.into()),
                text_color: Some(pair.text),
                border: iced::border::rounded(4),
                ..container::Style::default()
            }
        })
        .into()
}
