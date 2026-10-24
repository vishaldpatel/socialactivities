//! Small styling helpers shared by the views.

use iced::font::{Font, Weight};
use iced::widget::{Text, container, text};
use iced::{Element, Theme};

pub fn bold<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    text(s).font(Font {
        weight: Weight::Bold,
        ..Font::DEFAULT
    })
}

pub fn heading<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    bold(s).size(20)
}

pub fn muted<'a>(s: impl text::IntoFragment<'a>) -> Text<'a> {
    text(s).size(13).style(|theme: &Theme| text::Style {
        color: Some(theme.extended_palette().background.strongest.color),
    })
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
