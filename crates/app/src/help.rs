//! The help panel: what the app is for, how to get around, the keyboard
//! shortcuts, and how lists work. Opened with the "?" button at the bottom of
//! the sidebar, or F1.

use iced::widget::{
    Column, button, center, column, container, mouse_area, opaque, row, scrollable, space, text,
};
use iced::{Center, Color, Element, Fill, Theme};

use crate::app::Message;
use crate::{focusable, ui};

/// The help panel's scroll area, so the arrow keys scroll it while it's open.
pub const SCROLL: &str = "help-scroll";

/// Where people publish and find lists.
const LISTS_SITE: &str = "https://vishaldpatel.github.io/social-group-and-event-lists/";

const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+E / Ctrl+G / Ctrl+L", "Events / Groups / Lists"),
    ("Ctrl+N", "New event, group or list, depending on the panel"),
    ("Ctrl+S", "Jump to the search box"),
    ("Ctrl+R", "Jump into the panel on the right"),
    (
        "↑ ↓, Page Up/Down, Home, End",
        "Scroll the panel on the right",
    ),
    (
        "Tab / Shift+Tab",
        "Next / previous field, button, tag or checkbox",
    ),
    (
        "Enter or Space",
        "Press the highlighted button, or toggle the checkbox",
    ),
    ("↑ ↓ on a highlighted dropdown", "Previous / next option"),
    (
        "↓ in a date field",
        "Open the calendar; arrows pick a day, Enter chooses",
    ),
    ("Esc", "Close a calendar, dropdown or this help"),
    ("F1", "Open or close this help"),
];

/// The small "?" button that opens help.
pub fn button_view<'a>() -> Element<'a, Message> {
    let question = button(text("?").size(14).align_x(Center).width(Fill))
        .width(30)
        .padding([4, 0])
        .style(ui::gray);
    let label = container(text("Help (F1)").size(13))
        .padding([4, 8])
        .style(container::bordered_box);
    iced::widget::tooltip(
        focusable::button(question, Message::Help(true)),
        label,
        iced::widget::tooltip::Position::Top,
    )
    .gap(4)
    .into()
}

/// `base` with the help panel over it, dimmed behind. Clicking outside the
/// panel closes it.
pub fn overlay<'a>(base: Element<'a, Message>) -> Element<'a, Message> {
    let panel = container(
        column![
            row![
                ui::heading("Help"),
                space::horizontal(),
                focusable::button(button("Close").style(ui::gray), Message::Help(false)),
            ]
            .align_y(Center),
            scrollable(content().padding(iced::Padding::new(0.0).right(14)))
                .id(SCROLL)
                .height(Fill),
        ]
        .spacing(12),
    )
    .padding(20)
    .max_width(680)
    .style(|theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(palette.background.base.color.into()),
            border: iced::border::rounded(8)
                .color(palette.background.strong.color)
                .width(1),
            ..container::Style::default()
        }
    });

    // Clicks on the panel itself shouldn't count as clicks outside it.
    let panel = mouse_area(panel).on_press(Message::Noop);
    let backdrop = mouse_area(
        center(opaque(panel))
            .padding(32)
            .style(|_| container::background(Color::from_rgba(0.0, 0.0, 0.0, 0.5))),
    )
    .on_press(Message::Help(false));
    iced::widget::stack![base, opaque(backdrop)].into()
}

fn content<'a>() -> Column<'a, Message> {
    let section = |title| ui::bold(title).size(16);
    let para = |s| text(s).size(14);

    let shortcuts = Column::with_children(SHORTCUTS.iter().map(|(keys, does)| {
        row![
            text(*keys).size(14).width(230),
            text(*does).size(14).width(Fill)
        ]
        .spacing(12)
        .into()
    }))
    .spacing(6);

    column![
        section("What this is"),
        para(
            "A guide to groups and events around San Diego County where you take part \
             rather than watch: run clubs, pickup soccer, board game nights, trivia, \
             classes and more. Everything you see comes from lists. Some lists come with \
             the app, you can add others, and you can make and share your own."
        ),
        section("Getting around"),
        para(
            "Events shows what's coming up, grouped by day, week or month, as far ahead \
             as you choose. Groups shows who runs them. The search box, area, categories \
             and tags on the left narrow both. Shake 'n Bake is for active things, Chill \
             for relaxed ones. Tags that wouldn't match anything right now are grayed \
             out, but you can still pick them. Your filters are remembered next time."
        ),
        para(
            "On an event, the link button opens its page and the calendar button saves \
             it for your calendar app. On a group you're not interested in, Hide keeps it \
             and its events out of the way; the Hidden button in Groups brings it back."
        ),
        section("Keyboard shortcuts"),
        shortcuts,
        section("Lists"),
        para(
            "Every group and event belongs to a list. The Lists panel, at the bottom of \
             the sidebar, shows yours. Switch a list off to hide its groups and events \
             without deleting anything. Lists you added are read-only. They update \
             themselves once a week, or press Check for Updates to get changes sooner. \
             New lists published to the app's catalog are added \
             switched off; switch them on to try them."
        ),
        para(
            "To add a list someone shared, paste its link or choose its file under \
             \"Add a list\". Tick \"I maintain this list\" only if it's yours, so you can \
             edit it here."
        ),
        section("Making and sharing a list"),
        para(
            "Press New List, give it a name, then add groups and events. The editor's \
             \"Save to list\" chooses which of your lists they go in. When it's ready, \
             press Export… to save it as a file you can send to anyone."
        ),
        para(
            "To let people get your changes, put the file online and set its Update URL \
             (Lists → Edit) to that address before exporting. Everyone who added it can \
             then press Check for Updates. You can also offer it to the shared lists site, \
             where others find lists for their area."
        ),
        focusable::button(
            button(text("Open the lists site").size(14)).style(ui::gray),
            Message::OpenUrl(LISTS_SITE.into()),
        ),
        ui::muted(
            "New lists from that site arrive automatically. Lists from anywhere else \
             are added by link or file."
        ),
    ]
    .spacing(10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lists;

    #[test]
    fn the_lists_site_is_the_catalogs_home() {
        assert!(lists::INDEX_URL.starts_with(LISTS_SITE));
    }
}
