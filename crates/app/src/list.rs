//! The browsing views: upcoming events and groups.

use std::collections::BTreeSet;

use chrono::{Datelike, NaiveDate, TimeDelta};
use iced::widget::{Column, Row, button, column, image, pick_list, row, scrollable, space, text};
use iced::{Center, Element, Fill};
use sdsa_core::catalog::{category_name, neighborhood_name};
use sdsa_core::{Event, Group, Occurrence};

use crate::app::{App, Bucket, Horizon, Message};
use crate::{search, ui};

fn bucket_start(bucket: Bucket, date: NaiveDate) -> NaiveDate {
    match bucket {
        Bucket::Day => date,
        // Weeks start on Sunday.
        Bucket::Week => date - TimeDelta::days(i64::from(date.weekday().num_days_from_sunday())),
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

fn when(bucket: Bucket, occ: &Occurrence) -> String {
    if occ.end.date() > occ.start.date() + TimeDelta::days(1) {
        return format!(
            "{} – {}",
            occ.start.format("%b %-d"),
            occ.end.format("%b %-d")
        );
    }
    let time = occ.start.format("%-I:%M %p");
    match bucket {
        Bucket::Day => time.to_string(),
        _ => format!("{} · {time}", occ.start.format("%a %-d")),
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
        button(text(link_label(&l.kind)).size(13))
            .style(ui::gray)
            .on_press(Message::OpenUrl(l.url.clone()))
            .into()
    })
}

pub fn events(app: &App) -> Element<'_, Message> {
    let header = row![
        button("Add Event").on_press(Message::NewEvent(None)),
        space::horizontal(),
        pick_list(Bucket::ALL, Some(app.bucket), Message::Bucket),
        pick_list(Horizon::ALL, Some(app.horizon), Message::Horizon),
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

    column![header, scrollable(list.padding([0, 12])).height(Fill)]
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
    let area = neighborhood_name(&event.neighborhood);
    let cost = event.cost.to_string();
    let mut details = vec![area, cost.as_str()];
    if let Some(g) = group {
        details.insert(0, &g.name);
    }
    if !event.location.is_empty() && event.location != area {
        details.push(&event.location);
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
    if !event.url.is_empty() {
        actions = actions.push(
            button(text("Details").size(13))
                .style(ui::gray)
                .on_press(Message::OpenUrl(event.url.clone())),
        );
    } else if let Some(g) = group {
        actions = actions.extend(link_buttons(g).take(1));
    }
    if app.can_edit(event.source) {
        actions = actions.push(
            button(text("Edit").size(13))
                .style(button::text)
                .on_press(Message::EditEvent(event.id.clone())),
        );
    }

    ui::card(
        row![ui::bold(when(app.bucket, occ)).width(130), info, actions,]
            .spacing(12)
            .align_y(Center),
    )
}

pub fn groups(app: &App) -> Element<'_, Message> {
    let visible: Vec<&Group> = app
        .groups
        .iter()
        .filter(|g| app.filters.group(g, &app.hidden))
        .collect();

    let header = row![button("Add Group").on_press(Message::NewGroup)].align_y(Center);

    let list: Element<'_, Message> = if visible.is_empty() {
        ui::muted("No groups match your search.").into()
    } else {
        Column::with_children(visible.into_iter().map(|g| group_card(app, g)))
            .spacing(10)
            .padding([0, 12])
            .into()
    };

    column![header, scrollable(list).height(Fill)]
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

    let area = neighborhood_name(&g.neighborhood);
    let cost = g.cost.to_string();
    let mut facts = vec![area];
    if !g.address.is_empty() && g.address != area {
        facts.push(&g.address);
    }
    if !g.frequency.is_empty() {
        facts.push(&g.frequency);
    }
    facts.push(&cost);

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
    if app.is_admin() {
        body = body.push(ui::muted(match g.last_verified {
            Some(d) => format!("Last verified {d}"),
            None => "Never verified".into(),
        }));
    }

    let mut actions = row![].spacing(6).extend(link_buttons(g));
    actions = actions.push(space::horizontal());
    actions = actions.push(
        button(text(if hidden { "Unhide" } else { "Hide" }).size(13))
            .style(button::text)
            .on_press(Message::SetHidden(g.id.clone(), !hidden)),
    );
    if !hidden {
        actions = actions.push(
            button(text("+ Event").size(13))
                .style(button::text)
                .on_press(Message::NewEvent(Some(g.id.clone()))),
        );
    }
    if app.can_edit(g.source) {
        actions = actions.push(
            button(text("Edit").size(13))
                .style(button::text)
                .on_press(Message::EditGroup(g.id.clone())),
        );
    }
    body = body.push(actions);

    let content: Element<'a, Message> = match app.logo_path(g) {
        Some(path) => row![image(path).width(56).height(56), body]
            .spacing(12)
            .into(),
        None => body.into(),
    };
    ui::card(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weeks_start_on_sunday() {
        let thu = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        assert_eq!(
            bucket_start(Bucket::Week, thu),
            NaiveDate::from_ymd_opt(2026, 10, 18).unwrap()
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
        assert_eq!(when(Bucket::Day, &occ), "Jul 22 – Jul 26");
        let short = Occurrence {
            end: start + TimeDelta::hours(2),
            ..occ
        };
        assert_eq!(when(Bucket::Day, &short), "9:00 AM");
        assert_eq!(when(Bucket::Week, &short), "Thu 22 · 9:00 AM");
    }
}
