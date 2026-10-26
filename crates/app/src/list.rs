//! The browsing views: upcoming events and groups.

use std::collections::BTreeSet;

use chrono::{Datelike, NaiveDate, TimeDelta};
use iced::widget::{Column, Row, button, column, row, scrollable, space, text};
use iced::{Center, Element, Fill};
use sdsa_core::catalog::{category_name, neighborhood_name};
use sdsa_core::{CostKind, Event, Group, Occurrence};

use crate::app::{App, Bucket, Horizon, Message};
use crate::{focusable, location, search, ui};

fn bucket_start(bucket: Bucket, date: NaiveDate) -> NaiveDate {
    match bucket {
        Bucket::Day => date,
        // Weeks start on Monday.
        Bucket::Week => date - TimeDelta::days(i64::from(date.weekday().num_days_from_monday())),
        Bucket::Month => date.with_day(1).unwrap_or(date),
    }
}

fn bucket_label(bucket: Bucket, start: NaiveDate) -> String {
    match bucket {
        Bucket::Day => start.format("%A, %B %-d").to_string(),
        Bucket::Week => format!("Week of {}", start.format("%B %-d")),
        Bucket::Month => start.format("%B %Y").to_string(),
    }
}

/// The time panel's width: "12:00 PM" plus a margin either side. Longer
/// labels use two lines.
const WHEN_WIDTH: f32 = 104.0;

fn when(bucket: Bucket, occ: &Occurrence) -> String {
    if occ.end.date() > occ.start.date() + TimeDelta::days(1) {
        return format!(
            "{} –\n{}",
            occ.start.format("%b %-d"),
            occ.end.format("%b %-d")
        );
    }
    let time = occ.start.format("%-I:%M %p");
    match bucket {
        Bucket::Day => time.to_string(),
        _ => format!("{}\n{time}", occ.start.format("%a %-d")),
    }
}

fn link_label(kind: &str) -> String {
    let mut chars = kind.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Buttons for opening a group's links.
fn link_buttons(group: &Group) -> impl Iterator<Item = Element<'_, Message>> {
    group.links.iter().map(|l| {
        focusable::button(
            button(text(link_label(&l.kind)).size(13)).style(ui::gray),
            Message::OpenUrl(l.url.clone()),
        )
    })
}

pub fn events(app: &App) -> Element<'_, Message> {
    let header = row![
        crate::focusable::button_with_id(
            button("Add Event"),
            Message::NewEvent(None),
            crate::app::RESULTS_START,
        ),
        space::horizontal(),
        focusable::pick_list(&Bucket::ALL, app.bucket, Message::Bucket),
        focusable::pick_list(&Horizon::ALL, app.horizon, Message::Horizon),
    ]
    .spacing(8)
    .align_y(Center);

    let visible = app.occurrences.iter().filter_map(|occ| {
        let event = &app.events[occ.event];
        let group = app.group(event.group_id.as_deref());
        app.filters
            .event(event, group, &app.hidden)
            .then_some((occ, event, group))
    });

    let mut list = Column::new().spacing(6);
    let mut current = None;
    let mut any = false;
    for (occ, event, group) in visible {
        any = true;
        let start = bucket_start(app.bucket, occ.start.date());
        if current != Some(start) {
            current = Some(start);
            list = list.push(space::vertical().height(8));
            list = list.push(ui::bold(bucket_label(app.bucket, start)).size(16));
        }
        list = list.push(occurrence(app, occ, event, group));
    }
    if !any {
        list = list.push(ui::muted("No events match your search."));
    }

    column![
        header,
        scrollable(list.padding([0, 12]))
            .id(crate::app::MAIN_SCROLL)
            .height(Fill)
    ]
    .spacing(8)
    .into()
}

fn occurrence<'a>(
    app: &'a App,
    occ: &Occurrence,
    event: &'a Event,
    group: Option<&'a Group>,
) -> Element<'a, Message> {
    let needle = app.filters.search.as_str();
    // The neighborhood, in its region's color.
    let area = neighborhood_name(&event.neighborhood);
    let place = location::color_of(&event.neighborhood);
    let cost = event.cost.to_string();
    let mut details = vec![(area, place)];
    // Free is the norm, so only a cost worth knowing about is shown.
    if event.cost.kind != CostKind::Free {
        details.push((cost.as_str(), None));
    }
    if let Some(g) = group {
        details.insert(0, (&g.name, None));
    }
    if !event.location.is_empty() && event.location != area {
        details.push((&event.location, None));
    }

    let group_tags = group.into_iter().flat_map(|g| &g.tags);
    let title = labels(
        app,
        ui::rich_bold(ui::highlight(&event.title, needle)).into(),
        // Categories are left off event rows for now.
        [],
        event.tags.iter().chain(group_tags),
    )
    .wrap()
    .vertical_spacing(4);

    let mut info = column![
        title,
        ui::rich_muted(ui::highlight_joined(&details, needle))
    ]
    .spacing(4)
    .width(Fill);
    // The description isn't listed, so show the part of it that matched.
    if let Some(snippet) = search::snippet(&event.description, needle, 40) {
        info = info.push(ui::rich_muted(ui::highlight(&snippet, needle)));
    }

    let mut actions = row![].spacing(6);
    // The event's own page, else its group's first link.
    let link = if event.url.is_empty() {
        group
            .and_then(|g| g.links.first())
            .map(|l| (link_label(&l.kind), &l.url))
    } else {
        Some(("Details".to_string(), &event.url))
    };
    if let Some((label, url)) = link {
        actions = actions.push(ui::icon_button(
            &ui::LINK,
            label,
            Message::OpenUrl(url.clone()),
        ));
    }
    actions = actions.push(ui::icon_button(
        &ui::CALENDAR,
        "Add to Calendar",
        Message::AddToCalendar(event.id.clone()),
    ));
    if app.can_edit(&event.list_id) {
        actions = actions.push(focusable::button(
            button(text("Edit").size(13)).style(button::text),
            Message::EditEvent(event.id.clone()),
        ));
    }

    ui::card_with_side(
        ui::bold(when(app.bucket, occ)).align_x(Center),
        WHEN_WIDTH,
        // Multi-day events are colored by their start time too.
        crate::sky::color(occ.start.time()),
        row![info, actions].spacing(12).align_y(Center),
    )
}

pub fn groups(app: &App) -> Element<'_, Message> {
    let visible: Vec<&Group> = app
        .groups
        .iter()
        .filter(|g| app.filters.group(g, &app.hidden, app.viewing_hidden))
        .collect();

    let mut header = row![crate::focusable::button_with_id(
        button("Add Group"),
        Message::NewGroup,
        crate::app::RESULTS_START,
    )]
    .spacing(8)
    .align_y(Center);
    let hidden = app.hidden_count();
    if hidden > 0 {
        header = header.push(focusable::button(
            button(text(format!("Hidden ({hidden})"))).style(if app.viewing_hidden {
                button::primary
            } else {
                ui::gray
            }),
            Message::ViewHidden(!app.viewing_hidden),
        ));
    }

    let list: Element<'_, Message> = if visible.is_empty() {
        ui::muted(if app.viewing_hidden {
            "No hidden groups match your search."
        } else {
            "No groups match your search."
        })
        .into()
    } else {
        Column::with_children(visible.into_iter().map(|g| group_card(app, g)))
            .spacing(10)
            .padding([0, 12])
            .into()
    };

    column![
        header,
        scrollable(list).id(crate::app::MAIN_SCROLL).height(Fill)
    ]
    .spacing(12)
    .into()
}

/// A title followed by category chips and clickable tag chips.
fn labels<'a>(
    app: &'a App,
    title: Element<'a, Message>,
    categories: impl IntoIterator<Item = &'a String>,
    tags: impl IntoIterator<Item = &'a String>,
) -> Row<'a, Message> {
    // A group and its event may share labels.
    let categories: BTreeSet<&String> = categories.into_iter().collect();
    let tags: BTreeSet<&String> = tags.into_iter().collect();
    row![title]
        .extend(categories.into_iter().map(|c| ui::chip(category_name(c))))
        .extend(tags.into_iter().map(|t| {
            ui::tag(
                t,
                app.filters.tags.contains(t),
                Message::ToggleTag(t.clone()),
            )
        }))
        .spacing(6)
        .align_y(Center)
}

fn group_card<'a>(app: &'a App, g: &'a Group) -> Element<'a, Message> {
    let hidden = app.hidden.contains(&g.id);

    let needle = app.filters.search.as_str();
    let mut title = labels(
        app,
        ui::rich_bold(ui::highlight(&g.name, needle))
            .size(17)
            .into(),
        &g.categories,
        &g.tags,
    );
    if hidden {
        title = title.push(text("Hidden").size(12));
    }

    // The neighborhood, in its region's color.
    let area = neighborhood_name(&g.neighborhood);
    let place = location::color_of(&g.neighborhood);
    let cost = g.cost.to_string();
    let mut facts = vec![(area, place)];
    if !g.address.is_empty() && g.address != area {
        facts.push((&g.address, None));
    }
    if !g.frequency.is_empty() {
        facts.push((&g.frequency, None));
    }
    facts.push((&cost, None));

    let next = app
        .occurrences
        .iter()
        .find(|o| app.events[o.event].group_id.as_deref() == Some(g.id.as_str()))
        .map(|o| format!("Next: {}", o.start.format("%a %b %-d, %-I:%M %p")));

    let title = title.wrap().vertical_spacing(4);
    let mut body = column![title, ui::rich_muted(ui::highlight_joined(&facts, needle))].spacing(6);
    if !g.description.is_empty() {
        body = body.push(ui::rich(ui::highlight(&g.description, needle)).size(14));
    }
    if let Some(next) = next {
        body = body.push(text(next).size(14));
    }
    if app.can_edit(&g.list_id) {
        body = body.push(ui::muted(match g.last_verified {
            Some(d) => format!("Last verified {}", crate::form::format_date(d)),
            None => "Never verified".into(),
        }));
    }

    let mut actions = row![].spacing(6).extend(link_buttons(g));
    actions = actions.push(space::horizontal());
    actions = actions.push(focusable::button(
        button(text(if hidden { "Unhide" } else { "Hide" }).size(13)).style(button::text),
        Message::SetHidden(g.id.clone(), !hidden),
    ));
    if !hidden {
        actions = actions.push(focusable::button(
            button(text("+ Event").size(13)).style(button::text),
            Message::NewEvent(Some(g.id.clone())),
        ));
    }
    if app.can_edit(&g.list_id) {
        actions = actions.push(focusable::button(
            button(text("Edit").size(13)).style(button::text),
            Message::EditGroup(g.id.clone()),
        ));
    }
    body = body.push(actions);

    ui::card(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_start_on_monday() {
        let thu = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        assert_eq!(
            bucket_start(Bucket::Week, thu),
            NaiveDate::from_ymd_opt(2026, 10, 19).unwrap()
        );
        let sun = NaiveDate::from_ymd_opt(2026, 10, 25).unwrap();
        assert_eq!(
            bucket_start(Bucket::Week, sun),
            NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(),
            "Sunday ends the week"
        );
        assert_eq!(
            bucket_start(Bucket::Month, thu),
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
        );
    }

    #[test]
    fn occurrence_label_for_multi_day_events() {
        let start = NaiveDate::from_ymd_opt(2027, 7, 22)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        let occ = Occurrence {
            event: 0,
            start,
            end: start + TimeDelta::days(4),
        };
        assert_eq!(when(Bucket::Day, &occ), "Jul 22 –\nJul 26");
        let short = Occurrence {
            end: start + TimeDelta::hours(2),
            ..occ
        };
        assert_eq!(when(Bucket::Day, &short), "9:00 AM");
        assert_eq!(when(Bucket::Week, &short), "Thu 22\n9:00 AM");
    }
}
