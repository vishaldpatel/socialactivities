//! Small styling helpers shared by the views.

use iced::font::{Font, Weight};
use iced::widget::text::{Rich, Span};
use std::sync::LazyLock;

use iced::widget::{Text, button, container, rich_text, span, svg, text, tooltip};
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

/// Secondary text: the theme's strongest background shade, blended 40% toward
/// the normal text color so it stays readable while still receding.
fn muted_style(theme: &Theme) -> text::Style {
    let palette = theme.extended_palette();
    let (from, to) = (
        palette.background.strongest.color,
        palette.background.base.text,
    );
    let mix = |a: f32, b: f32| a + (b - a) * 0.4;
    text::Style {
        color: Some(Color {
            r: mix(from.r, to.r),
            g: mix(from.g, to.g),
            b: mix(from.b, to.b),
            a: 1.0,
        }),
    }
}

pub fn muted<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    text(s).size(13).style(muted_style)
}

/// Small text in the theme's warning color, for consequences worth knowing.
pub fn warning<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    text(s).size(13).style(|theme: &Theme| text::Style {
        color: Some(theme.extended_palette().warning.base.color),
    })
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
    highlight_colored(s, needle, None)
}

/// Like [`highlight`], with the text in `color` if given.
fn highlight_colored<'a>(s: &str, needle: &str, color: Option<Color>) -> Vec<Span<'a, (), Font>> {
    let piece = |t: &str| span(t.to_string()).color_maybe(color);
    let mut spans = Vec::new();
    let mut last = 0;
    for m in search::find(s, needle) {
        if m.start > last {
            spans.push(piece(&s[last..m.start]));
        }
        spans.push(piece(&s[m.clone()]).background(HIGHLIGHT));
        last = m.end;
    }
    if last < s.len() || spans.is_empty() {
        spans.push(piece(&s[last..]));
    }
    spans
}

/// Several fields on one line, separated by " · ", each highlighted and in
/// its own color if it has one.
pub fn highlight_joined<'a>(
    parts: &[(&str, Option<Color>)],
    needle: &str,
) -> Vec<Span<'a, (), Font>> {
    let mut spans = Vec::new();
    for (i, (part, color)) in parts.iter().enumerate() {
        if i > 0 {
            spans.push(span(" · "));
        }
        spans.extend(highlight_colored(part, needle, *color));
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

/// Gray buttons: Iced's `secondary` puts dark text on mid-gray, which is hard
/// to read in the dark theme, so this uses a darker gray with the theme's light
/// text instead.
pub fn gray(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let pair = match status {
        button::Status::Hovered | button::Status::Pressed => palette.background.strongest,
        _ => palette.background.strong,
    };
    let style = button::Style {
        background: Some(pair.color.into()),
        text_color: palette.background.base.text,
        border: iced::border::rounded(2),
        ..button::Style::default()
    };
    match status {
        button::Status::Disabled => button::Style {
            text_color: palette.background.strongest.color,
            ..style
        },
        _ => style,
    }
}

/// A calendar with a plus sign.
pub static CALENDAR: LazyLock<svg::Handle> =
    LazyLock::new(|| svg::Handle::from_memory(include_bytes!("../assets/calendar-plus.svg")));

/// Two chain links.
pub static LINK: LazyLock<svg::Handle> =
    LazyLock::new(|| svg::Handle::from_memory(include_bytes!("../assets/link.svg")));

/// A small gray button showing `icon`, the same height as a size-13 text
/// button, with `label` as a tooltip on hover.
pub fn icon_button<'a, M: Clone + 'a>(
    icon: &svg::Handle,
    label: impl text::IntoFragment<'a>,
    msg: M,
) -> Element<'a, M> {
    let icon = svg(icon.clone())
        .width(17)
        .height(17)
        .style(|theme: &Theme, _| svg::Style {
            color: Some(theme.extended_palette().background.base.text),
        });
    tooltip(
        crate::focusable::button(button(icon).padding([5, 7]).style(gray), msg),
        container(text(label).size(13))
            .padding([4, 8])
            .style(container::bordered_box),
        tooltip::Position::Top,
    )
    .gap(4)
    .into()
}

/// A clickable tag chip; `selected` when it's part of the current search.
pub fn tag<'a, M: Clone + 'a>(label: &str, selected: bool, on_press: M) -> Element<'a, M> {
    filter_tag(label, selected, 1.0, on_press)
}

/// A tag in the search panel. One that wouldn't match anything right now is
/// grayed out, but can still be picked. `presence` runs from grayed out (0)
/// to normal (1), so the change can fade.
pub fn filter_tag<'a, M: Clone + 'a>(
    label: &str,
    selected: bool,
    presence: f32,
    on_press: M,
) -> Element<'a, M> {
    let chip = button(text(label.to_string()).size(12))
        .padding([2, 8])
        .style(move |theme: &Theme, status| {
            let palette = theme.extended_palette();
            let (normal, grayed) = if selected {
                let normal = button::primary(theme, status);
                let grayed = button::Style {
                    background: Some(palette.primary.weak.color.into()),
                    text_color: palette.primary.weak.text,
                    ..normal
                };
                (normal, grayed)
            } else {
                let normal = gray(theme, status);
                let grayed = button::Style {
                    text_color: palette.background.strongest.color,
                    ..normal
                };
                (normal, grayed)
            };
            let background = |style: &button::Style| match style.background {
                Some(iced::Background::Color(c)) => c,
                _ => Color::TRANSPARENT,
            };
            button::Style {
                background: Some(mix(background(&grayed), background(&normal), presence).into()),
                text_color: mix(grayed.text_color, normal.text_color, presence),
                border: iced::border::rounded(10),
                ..normal
            }
        });
    crate::focusable::button(chip, on_press)
}

/// The color `t` of the way from `a` to `b`.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    Color::from_rgba(
        lerp(a.r, b.r),
        lerp(a.g, b.g),
        lerp(a.b, b.b),
        lerp(a.a, b.a),
    )
}

pub fn card<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(12)
        .width(iced::Fill)
        .style(container::bordered_box)
        .into()
}

/// A card with a colored, full-height panel down its left edge (an event's
/// time, on its time-of-day sky color), its content centered so the space
/// either side is equal. The panel's text is white.
pub fn card_with_side<'a, M: 'a>(
    side: impl Into<Element<'a, M>>,
    side_width: f32,
    side_color: Color,
    body: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    let side = container(side)
        .center_x(side_width)
        .center_y(iced::Fill)
        .style(move |_: &Theme| container::Style {
            background: Some(side_color.into()),
            text_color: Some(Color::WHITE),
            // Just inside the card's 5px corners and 1px border.
            border: iced::border::rounded(iced::border::left(4)),
            ..container::Style::default()
        });
    container(iced::widget::row![
        side,
        container(body).padding(12).width(iced::Fill)
    ])
    // Leaves the card's border visible around the panel.
    .padding(1)
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

pub fn banner<'a, M: 'a>(message: impl text::IntoFragment<'a>, is_error: bool) -> Element<'a, M> {
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
