//! Add/edit forms for groups and events.

use std::collections::{BTreeSet, HashSet};

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use iced::widget::{
    Column, button, column, pick_list, row, scrollable, space, text, text_editor, text_input,
};
use iced::{Center, Element, Fill};
use sdsa_core::catalog::{CATEGORIES, LINK_KINDS, Neighborhood, find_neighborhood, normalize_tag};
use sdsa_core::recurrence::{self, Repeat};
use sdsa_core::{Cost, CostKind, Event, Group, Link, ListInfo};

use crate::app::{App, Message};
use crate::{calendar, focusable};
use crate::{location, ui};

/// The first field of each form, focused when the form opens.
pub const FIRST_INPUT: &str = "form-first";

/// The event form's date field, refocused when the calendar closes.
pub const DATE_INPUT: &str = "event-date";

/// A single edit to whichever form is open.
#[derive(Debug, Clone)]
pub enum Field {
    Name(String),
    Description(text_editor::Action),
    Neighborhood(Neighborhood),
    Place(String),
    CostKind(CostKind),
    CostNote(String),
    Category(&'static str, bool),
    Tags(String),
    /// Appends an existing tag to the tags field.
    AddTag(String),
    // Groups
    Logo(String),
    Frequency(String),
    LinkKind(usize, &'static str),
    LinkUrl(usize, String),
    AddLink,
    RemoveLink(usize),
    Verified(String),
    VerifiedToday,
    List(ListChoice),
    // Events
    Group(GroupChoice),
    Date(String),
    Time(String),
    Hours(String),
    Repeat(Repeat),
    Rule(String),
    Url(String),
    Cancelled(String),
    OpenCalendar,
    Calendar(calendar::Action),
}

/// An entry in the list picker: one of your lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListChoice {
    id: String,
    name: String,
}

impl std::fmt::Display for ListChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

impl ListChoice {
    fn of(list: &ListInfo) -> Self {
        Self {
            id: list.id.clone(),
            name: list.name.clone(),
        }
    }
}

/// An entry in the event form's group picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupChoice {
    id: Option<String>,
    name: String,
}

impl std::fmt::Display for GroupChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

impl GroupChoice {
    fn none() -> Self {
        Self {
            id: None,
            name: "No group (one-off / special event)".into(),
        }
    }

    fn of(g: &Group) -> Self {
        Self {
            id: Some(g.id.clone()),
            name: g.name.clone(),
        }
    }
}

pub enum Record {
    Group(Group),
    Event(Event),
}

pub enum Editor {
    Group(GroupForm),
    Event(EventForm),
}

/// State shared by both forms.
struct Common {
    is_new: bool,
    /// The list it's saved to; always one you own.
    list_id: String,
    description: text_editor::Content,
    /// `None` until one is chosen. Existing countywide records keep that value,
    /// though it can no longer be picked.
    neighborhood: Option<Neighborhood>,
    place: String,
    cost_kind: CostKind,
    cost_note: String,
    categories: BTreeSet<String>,
    /// Free-form tags as typed, comma separated.
    tags: String,
    error: Option<String>,
    confirm_delete: bool,
}

impl Common {
    fn new(
        is_new: bool,
        list_id: &str,
        description: &str,
        neighborhood: &str,
        place: &str,
        cost: &Cost,
        (categories, tags): (&[String], &[String]),
    ) -> Self {
        Self {
            is_new,
            list_id: list_id.into(),
            description: text_editor::Content::with_text(description),
            neighborhood: find_neighborhood(neighborhood),
            place: place.into(),
            cost_kind: cost.kind,
            cost_note: cost.note.clone(),
            categories: categories.iter().cloned().collect(),
            tags: tags.join(", "),
            error: None,
            confirm_delete: false,
        }
    }

    /// Applies a shared field; returns `false` if the field belongs to a specific form.
    fn update(&mut self, field: &Field) -> bool {
        match field {
            Field::Description(action) => self.description.perform(action.clone()),
            Field::Neighborhood(n) => self.neighborhood = Some(*n),
            Field::List(choice) => self.list_id = choice.id.clone(),
            Field::Place(s) => self.place = s.clone(),
            Field::CostKind(k) => self.cost_kind = *k,
            Field::CostNote(s) => self.cost_note = s.clone(),
            Field::Category(slug, true) => {
                self.categories.insert(slug.to_string());
            }
            Field::Category(slug, false) => {
                self.categories.remove(*slug);
            }
            Field::Tags(s) => self.tags = s.clone(),
            Field::AddTag(tag) => {
                if !self.tags().contains(tag) {
                    let current = self.tags.trim().trim_end_matches(',');
                    self.tags = if current.is_empty() {
                        tag.clone()
                    } else {
                        format!("{current}, {tag}")
                    };
                }
            }
            _ => return false,
        }
        self.confirm_delete = false;
        true
    }

    fn cost(&self) -> Cost {
        Cost {
            kind: self.cost_kind,
            note: self.cost_note.trim().into(),
        }
    }

    fn neighborhood_slug(&self) -> Result<String, String> {
        self.neighborhood
            .map(|n| n.slug.to_string())
            .ok_or_else(|| "Choose a neighborhood.".to_string())
    }

    fn description(&self) -> String {
        self.description.text().trim().into()
    }

    fn categories(&self) -> Vec<String> {
        self.categories.iter().cloned().collect()
    }

    /// The typed tags, normalized, deduplicated and sorted.
    fn tags(&self) -> Vec<String> {
        let tags: BTreeSet<String> = self.tags.split(',').filter_map(normalize_tag).collect();
        tags.into_iter().collect()
    }
}

pub struct GroupForm {
    original: Group,
    common: Common,
    name: String,
    logo: String,
    frequency: String,
    links: Vec<(&'static str, String)>,
    verified: String,
}

impl GroupForm {
    pub fn new(g: Group, is_new: bool) -> Self {
        let links = g
            .links
            .iter()
            .map(|l| {
                let kind = LINK_KINDS
                    .iter()
                    .copied()
                    .find(|k| *k == l.kind)
                    .unwrap_or("other");
                (kind, l.url.clone())
            })
            .collect();
        Self {
            common: Common::new(
                is_new,
                &g.list_id,
                &g.description,
                &g.neighborhood,
                &g.address,
                &g.cost,
                (&g.categories, &g.tags),
            ),
            name: g.name.clone(),
            logo: g.logo_path.clone().unwrap_or_default(),
            frequency: g.frequency.clone(),
            links,
            verified: g.last_verified.map(format_date).unwrap_or_default(),
            original: g,
        }
    }

    fn update(&mut self, field: Field) {
        if self.common.update(&field) {
            return;
        }
        match field {
            Field::Name(s) => self.name = s,
            Field::Logo(s) => self.logo = s,
            Field::Frequency(s) => self.frequency = s,
            Field::LinkKind(i, kind) => {
                if let Some(link) = self.links.get_mut(i) {
                    link.0 = kind;
                }
            }
            Field::LinkUrl(i, url) => {
                if let Some(link) = self.links.get_mut(i) {
                    link.1 = url;
                }
            }
            Field::AddLink => self.links.push(("website", String::new())),
            Field::RemoveLink(i) => {
                if i < self.links.len() {
                    self.links.remove(i);
                }
            }
            Field::Verified(s) => self.verified = s,
            Field::VerifiedToday => self.verified = format_date(recurrence::now_local().date()),
            _ => {}
        }
        self.common.confirm_delete = false;
    }

    fn build(&self) -> Result<Group, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Name is required.".into());
        }
        let verified = match self.verified.trim() {
            "" => None,
            s => Some(parse_date(s).ok_or("Last verified must look like 10-23-2026.")?),
        };
        let logo = self.logo.trim();
        Ok(Group {
            name: name.into(),
            description: self.common.description(),
            logo_path: (!logo.is_empty()).then(|| logo.into()),
            neighborhood: self.common.neighborhood_slug()?,
            address: self.common.place.trim().into(),
            frequency: self.frequency.trim().into(),
            cost: self.common.cost(),
            categories: self.common.categories(),
            tags: self.common.tags(),
            links: self
                .links
                .iter()
                .filter(|(_, url)| !url.trim().is_empty())
                .map(|(kind, url)| Link {
                    kind: kind.to_string(),
                    url: url.trim().into(),
                })
                .collect(),
            last_verified: verified,
            list_id: self.common.list_id.clone(),
            ..self.original.clone()
        })
    }

    fn view<'a>(&'a self, all_tags: &'a [String]) -> Column<'a, Message> {
        let links = Column::with_children(self.links.iter().enumerate().map(|(i, (kind, url))| {
            row![
                pick_list(LINK_KINDS, Some(*kind), move |k| Message::Form(
                    Field::LinkKind(i, k)
                ))
                .width(130),
                text_input("https://…", url)
                    .on_input(move |s| Message::Form(Field::LinkUrl(i, s)))
                    .width(Fill),
                press(
                    button("Remove").style(button::text),
                    Message::Form(Field::RemoveLink(i))
                ),
            ]
            .spacing(8)
            .align_y(Center)
            .into()
        }))
        .spacing(6)
        .push(press(
            button("+ Add Link").style(ui::gray),
            Message::Form(Field::AddLink),
        ));

        column![
            labeled(
                "Name",
                text_input("e.g. PB Sunset Volleyball", &self.name)
                    .id(FIRST_INPUT)
                    .on_input(|s| Message::Form(Field::Name(s)))
            ),
            description(&self.common),
            row![
                labeled("Neighborhood", neighborhood_picker(&self.common)),
                labeled(
                    "Home base / address",
                    text_input("Where they usually meet", &self.common.place)
                        .on_input(|s| Message::Form(Field::Place(s)))
                ),
            ]
            .spacing(12),
            labeled(
                "How often they meet",
                text_input("e.g. Weekly on Saturdays", &self.frequency)
                    .on_input(|s| Message::Form(Field::Frequency(s)))
            ),
            cost(&self.common),
            categories(&self.common),
            tags(&self.common, all_tags),
            labeled("Links", links),
            labeled(
                "Logo image path (relative to the database folder, or absolute)",
                text_input("logos/my-group.png", &self.logo)
                    .on_input(|s| Message::Form(Field::Logo(s)))
            ),
            labeled(
                "Last verified (MM-DD-YYYY)",
                row![
                    text_input("", &self.verified)
                        .on_input(|s| Message::Form(Field::Verified(s)))
                        .width(160),
                    press(
                        button("Today").style(ui::gray),
                        Message::Form(Field::VerifiedToday)
                    ),
                ]
                .spacing(8)
            ),
        ]
    }
}

pub struct EventForm {
    original: Event,
    common: Common,
    title: String,
    group: GroupChoice,
    date: String,
    time: String,
    hours: String,
    repeat: Repeat,
    rule: String,
    url: String,
    cancelled: String,
    /// The highlighted day while the date calendar is open.
    calendar: Option<NaiveDate>,
}

impl EventForm {
    pub fn new(e: Event, is_new: bool) -> Self {
        let repeat = Repeat::detect(e.start, e.rrule.as_deref());
        Self {
            common: Common::new(
                is_new,
                &e.list_id,
                &e.description,
                &e.neighborhood,
                &e.location,
                &e.cost,
                (&e.categories, &e.tags),
            ),
            title: e.title.clone(),
            // The real name is filled in by `view`, which has the group list.
            group: GroupChoice {
                id: e.group_id.clone(),
                name: String::new(),
            },
            date: format_date(e.start.date()),
            time: e.start.format("%H:%M").to_string(),
            hours: format_hours(e.duration_minutes),
            repeat,
            rule: e.rrule.clone().unwrap_or_default(),
            url: e.url.clone(),
            cancelled: e
                .cancelled
                .iter()
                .map(|d| format_date(*d))
                .collect::<Vec<_>>()
                .join(", "),
            calendar: None,
            original: e,
        }
    }

    /// Selects `g` and, for new events, copies its location and cost as a starting point.
    pub fn choose_group(&mut self, g: &Group) {
        self.group = GroupChoice::of(g);
        if self.common.is_new {
            self.common.neighborhood = find_neighborhood(&g.neighborhood);
            self.common.place = g.address.clone();
            self.common.cost_kind = g.cost.kind;
            self.common.cost_note = g.cost.note.clone();
            if self.title.is_empty() {
                self.title = g.name.clone();
            }
        }
    }

    fn update(&mut self, field: Field, groups: &[Group]) {
        if self.common.update(&field) {
            return;
        }
        match field {
            Field::Name(s) => self.title = s,
            Field::Group(choice) => match groups.iter().find(|g| Some(&g.id) == choice.id.as_ref())
            {
                Some(g) => self.choose_group(g),
                None => self.group = GroupChoice::none(),
            },
            Field::Date(s) => self.date = s,
            Field::OpenCalendar => {
                let today = recurrence::now_local().date();
                self.calendar = Some(parse_date(self.date.trim()).unwrap_or(today));
            }
            Field::Calendar(action) => match action {
                calendar::Action::Pick(day) => {
                    self.date = format_date(day);
                    self.calendar = None;
                }
                calendar::Action::Close => self.calendar = None,
                _ => self.calendar = self.calendar.map(|c| calendar::moved(c, action)),
            },
            Field::Time(s) => self.time = s,
            Field::Hours(s) => self.hours = s,
            Field::Repeat(r) => {
                // Seed the custom rule with whatever the preset would have produced.
                if r == Repeat::Custom
                    && self.rule.is_empty()
                    && let Ok(start) = self.start()
                {
                    self.rule = self.repeat.to_rrule(start).unwrap_or_default();
                }
                self.repeat = r;
            }
            Field::Rule(s) => self.rule = s,
            Field::Url(s) => self.url = s,
            Field::Cancelled(s) => self.cancelled = s,
            _ => {}
        }
        self.common.confirm_delete = false;
    }

    fn start(&self) -> Result<NaiveDateTime, String> {
        let date = parse_date(self.date.trim()).ok_or("Date must look like 10-23-2026.")?;
        let time = parse_time(self.time.trim()).ok_or("Time must look like 19:00 or 7:00 PM.")?;
        Ok(date.and_time(time))
    }

    fn rrule(&self, start: NaiveDateTime) -> Result<Option<String>, String> {
        let rule = match self.repeat {
            Repeat::Custom => {
                let rule = self.rule.trim().trim_start_matches("RRULE:").trim();
                if rule.is_empty() {
                    return Err("Enter a custom RRULE, e.g. FREQ=WEEKLY;BYDAY=MO,WE.".into());
                }
                Some(rule.to_string())
            }
            preset => preset.to_rrule(start),
        };
        if let Some(rule) = &rule {
            recurrence::validate(start, rule).map_err(|e| e.to_string())?;
        }
        Ok(rule)
    }

    fn build(&self) -> Result<Event, String> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err("Title is required.".into());
        }
        let start = self.start()?;
        let hours: f64 = self
            .hours
            .trim()
            .parse()
            .ok()
            .filter(|h: &f64| *h > 0.0 && *h <= 24.0 * 31.0)
            .ok_or("Duration must be a number of hours, e.g. 2 or 1.5.")?;
        let cancelled = self
            .cancelled
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| {
                parse_date(s).ok_or(format!("Cancelled date {s:?} must look like 10-23-2026."))
            })
            .collect::<Result<_, _>>()?;
        Ok(Event {
            group_id: self.group.id.clone(),
            title: title.into(),
            description: self.common.description(),
            neighborhood: self.common.neighborhood_slug()?,
            location: self.common.place.trim().into(),
            start,
            duration_minutes: (hours * 60.0).round() as u32,
            rrule: self.rrule(start)?,
            cost: self.common.cost(),
            url: self.url.trim().into(),
            categories: self.common.categories(),
            tags: self.common.tags(),
            cancelled,
            list_id: self.common.list_id.clone(),
            ..self.original.clone()
        })
    }

    /// A description of the schedule as entered, or why it's invalid.
    fn preview(&self) -> String {
        let built = self.start().and_then(|start| {
            let mut e = Event::new(&self.common.list_id, start);
            e.rrule = self.rrule(start)?;
            Ok(e)
        });
        match built {
            Ok(e) => recurrence::describe(&e),
            Err(msg) => msg,
        }
    }

    /// The group picker's options. Hidden groups can't be chosen, but an event
    /// already in one keeps showing it.
    fn group_choices(&self, groups: &[Group], hidden: &HashSet<String>) -> Vec<GroupChoice> {
        std::iter::once(GroupChoice::none())
            .chain(
                groups
                    .iter()
                    .filter(|g| !hidden.contains(&g.id) || self.group.id.as_ref() == Some(&g.id))
                    .map(GroupChoice::of),
            )
            .collect()
    }

    fn view<'a>(
        &'a self,
        groups: &'a [Group],
        hidden: &HashSet<String>,
        all_tags: &'a [String],
    ) -> Column<'a, Message> {
        let choices = self.group_choices(groups, hidden);
        let selected = choices
            .iter()
            .find(|c| c.id == self.group.id)
            .cloned()
            .unwrap_or_else(GroupChoice::none);

        let mut schedule = column![
            row![
                labeled(
                    "When (MM-DD-YYYY)",
                    row![
                        text_input("10-23-2026", &self.date)
                            .id(DATE_INPUT)
                            .on_input(|s| Message::Form(Field::Date(s))),
                        press(
                            button(text("Pick…").size(14)).style(ui::gray),
                            Message::Form(Field::OpenCalendar)
                        ),
                    ]
                    .spacing(6)
                    .align_y(Center)
                ),
                labeled(
                    "Start time",
                    text_input("19:00", &self.time).on_input(|s| Message::Form(Field::Time(s)))
                ),
                labeled(
                    "Duration (hours)",
                    text_input("2", &self.hours).on_input(|s| Message::Form(Field::Hours(s)))
                ),
                labeled(
                    "Repeats",
                    pick_list(Repeat::ALL, Some(self.repeat), |r| Message::Form(
                        Field::Repeat(r)
                    ))
                ),
            ]
            .spacing(12),
        ]
        .spacing(8);
        if let Some(cursor) = self.calendar {
            schedule = schedule.push(calendar::view(cursor, parse_date(self.date.trim()), |a| {
                Message::Form(Field::Calendar(a))
            }));
        }
        if self.repeat == Repeat::Custom {
            schedule = schedule.push(labeled(
                "RRULE (RFC 5545)",
                text_input("FREQ=WEEKLY;BYDAY=MO,WE", &self.rule)
                    .on_input(|s| Message::Form(Field::Rule(s))),
            ));
        }
        schedule = schedule.push(ui::muted(format!(
            "{} — San Diego local time",
            self.preview()
        )));
        if self.repeat != Repeat::Once {
            schedule = schedule.push(labeled(
                "Cancelled dates (comma separated)",
                text_input("12-25-2026, 01-01-2027", &self.cancelled)
                    .on_input(|s| Message::Form(Field::Cancelled(s))),
            ));
        }

        column![
            labeled(
                "Title",
                text_input("e.g. Tuesday game night", &self.title)
                    .id(FIRST_INPUT)
                    .on_input(|s| Message::Form(Field::Name(s)))
            ),
            labeled(
                "Group",
                pick_list(choices, Some(selected), |c| Message::Form(Field::Group(c))).width(Fill)
            ),
            description(&self.common),
            schedule,
            row![
                labeled("Neighborhood", neighborhood_picker(&self.common)),
                labeled(
                    "Location",
                    text_input("Venue or meeting spot", &self.common.place)
                        .on_input(|s| Message::Form(Field::Place(s)))
                ),
            ]
            .spacing(12),
            cost(&self.common),
            categories(&self.common),
            tags(&self.common, all_tags),
            labeled(
                "Link",
                text_input("https://…", &self.url).on_input(|s| Message::Form(Field::Url(s)))
            ),
        ]
    }
}

impl Editor {
    fn common(&self) -> &Common {
        match self {
            Editor::Group(f) => &f.common,
            Editor::Event(f) => &f.common,
        }
    }

    fn common_mut(&mut self) -> &mut Common {
        match self {
            Editor::Group(f) => &mut f.common,
            Editor::Event(f) => &mut f.common,
        }
    }

    pub fn update(&mut self, field: Field, groups: &[Group]) {
        match self {
            Editor::Group(f) => f.update(field),
            Editor::Event(f) => f.update(field, groups),
        }
    }

    /// The calendar's action for `key`, if a calendar is open and uses it.
    pub fn calendar_action(&self, key: &iced::keyboard::Key) -> Option<calendar::Action> {
        match self {
            Editor::Event(EventForm {
                calendar: Some(cursor),
                ..
            }) => calendar::key_action(key, *cursor),
            _ => None,
        }
    }

    pub fn build(&self) -> Result<Record, String> {
        match self {
            Editor::Group(f) => f.build().map(Record::Group),
            Editor::Event(f) => f.build().map(Record::Event),
        }
    }

    /// The record as it was before editing, for deletion.
    pub fn original(&self) -> Record {
        match self {
            Editor::Group(f) => Record::Group(f.original.clone()),
            Editor::Event(f) => Record::Event(f.original.clone()),
        }
    }

    pub fn set_error(&mut self, msg: String) {
        self.common_mut().error = Some(msg);
    }

    /// Returns whether deletion was already confirmed; otherwise asks for confirmation.
    pub fn confirm_delete(&mut self) -> bool {
        let common = self.common_mut();
        if common.confirm_delete {
            return true;
        }
        common.confirm_delete = true;
        false
    }

    pub fn view<'a>(&'a self, app: &'a App) -> Element<'a, Message> {
        let common = self.common();
        let save = match (self, common.is_new) {
            (Editor::Event(_), true) => "Create Event",
            (Editor::Event(_), false) => "Update Event",
            (Editor::Group(_), true) => "Create Group",
            (Editor::Group(_), false) => "Update Group",
        };

        let fields = match self {
            Editor::Group(f) => f.view(&app.tags),
            Editor::Event(f) => f.view(&app.groups, &app.hidden, &app.tags),
        };
        let lists: Vec<ListChoice> = app.owned_lists().map(ListChoice::of).collect();
        let selected = lists.iter().find(|l| l.id == common.list_id).cloned();
        let fields = fields.push(labeled(
            "Save to list",
            pick_list(lists, selected, |l| Message::Form(Field::List(l))).width(Fill),
        ));

        let mut actions = row![
            press(button(save).style(button::primary), Message::Save),
            press(button("Cancel").style(ui::gray), Message::Cancel),
            space::horizontal(),
        ]
        .spacing(8)
        .align_y(Center);
        if !common.is_new {
            if common.confirm_delete {
                actions = actions.push(text("Delete permanently?").size(14));
            }
            let label = if common.confirm_delete {
                "Yes, Delete"
            } else {
                "Delete"
            };
            actions = actions.push(press(button(label).style(button::danger), Message::Delete));
        }

        let mut form = fields.spacing(14).padding([0, 12]);
        if let Some(err) = &common.error {
            form = form.push(ui::banner(err, true));
        }
        form = form.push(actions);

        scrollable(form)
            .id(crate::app::MAIN_SCROLL)
            .height(Fill)
            .into()
    }
}

fn labeled<'a>(label: &'a str, input: impl Into<Element<'a, Message>>) -> Column<'a, Message> {
    column![ui::muted(label), input.into()]
        .spacing(4)
        .width(Fill)
}

fn description(common: &Common) -> Column<'_, Message> {
    labeled(
        "Description",
        text_editor(&common.description)
            .on_action(|a| Message::Form(Field::Description(a)))
            .height(110),
    )
}

fn neighborhood_picker(common: &Common) -> Element<'_, Message> {
    location::place_picker(common.neighborhood, |n| {
        Message::Form(Field::Neighborhood(n))
    })
}

fn cost(common: &Common) -> Element<'_, Message> {
    row![
        labeled(
            "Cost",
            pick_list(CostKind::ALL, Some(common.cost_kind), |k| Message::Form(
                Field::CostKind(k)
            ))
        )
        .width(180),
        labeled(
            "Cost details",
            text_input("e.g. $5 drop-in, $30/year", &common.cost_note)
                .on_input(|s| Message::Form(Field::CostNote(s)))
        ),
    ]
    .spacing(12)
    .into()
}

fn categories(common: &Common) -> Column<'_, Message> {
    let boxes = row(CATEGORIES.iter().map(|c| {
        focusable::checkbox(common.categories.contains(c.slug), c.name, move |on| {
            Message::Form(Field::Category(c.slug, on))
        })
    }))
    .spacing(14)
    .wrap();
    labeled("Categories", boxes)
}

/// The tags field, plus existing tags not yet used here as one-click suggestions.
fn tags<'a>(common: &'a Common, all_tags: &'a [String]) -> Column<'a, Message> {
    let current = common.tags();
    let suggestions = all_tags
        .iter()
        .filter(|t| !current.contains(t))
        .map(|t| ui::tag(t, false, Message::Form(Field::AddTag(t.clone()))));
    let mut field = column![
        text_input("e.g. beginner friendly, dog friendly", &common.tags)
            .on_input(|s| Message::Form(Field::Tags(s)))
    ]
    .spacing(6);
    if all_tags.iter().any(|t| !current.contains(t)) {
        field = field.push(row(suggestions).spacing(6).wrap().vertical_spacing(6));
    }
    labeled("Tags (comma separated; click one below to add it)", field)
}

/// Dates are shown and typed as MM-DD-YYYY. Slashes and ISO (YYYY-MM-DD,
/// e.g. pasted) are accepted too.
fn parse_date(s: &str) -> Option<NaiveDate> {
    ["%m-%d-%Y", "%m/%d/%Y", "%Y-%m-%d"]
        .iter()
        .find_map(|fmt| NaiveDate::parse_from_str(s, fmt).ok())
}

pub fn format_date(d: NaiveDate) -> String {
    d.format("%m-%d-%Y").to_string()
}

/// A button Tab can reach and Enter can press; `msg` is sent either way.
fn press<'a>(b: iced::widget::Button<'a, Message>, msg: Message) -> Element<'a, Message> {
    focusable::button(b, msg)
}

fn parse_time(s: &str) -> Option<NaiveTime> {
    let mut s = s.to_uppercase();
    // chrono needs minutes, so "7 PM" becomes "7:00 PM".
    if !s.contains(':') {
        let hour_len = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        s.insert_str(hour_len, ":00");
    }
    ["%H:%M", "%I:%M %p", "%I:%M%p"]
        .iter()
        .find_map(|fmt| NaiveTime::parse_from_str(&s, fmt).ok())
}

fn format_hours(minutes: u32) -> String {
    let hours = f64::from(minutes) / 60.0;
    if hours.fract() == 0.0 {
        format!("{hours:.0}")
    } else {
        format!("{hours:.2}").trim_end_matches('0').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_time_formats() {
        let seven_pm = NaiveTime::from_hms_opt(19, 0, 0);
        assert_eq!(parse_time("19:00"), seven_pm);
        assert_eq!(parse_time("7:00 pm"), seven_pm);
        assert_eq!(parse_time("7:00PM"), seven_pm);
        assert_eq!(parse_time("7 PM"), seven_pm);
        assert_eq!(parse_time("later"), None);
    }

    #[test]
    fn dates_are_month_day_year() {
        let d = NaiveDate::from_ymd_opt(2026, 3, 7).unwrap();
        assert_eq!(format_date(d), "03-07-2026");
        assert_eq!(parse_date("03-07-2026"), Some(d));
        assert_eq!(parse_date("3/7/2026"), Some(d));
        assert_eq!(parse_date("2026-03-07"), Some(d), "ISO still accepted");
        assert_eq!(parse_date("13-07-2026"), None);
    }

    #[test]
    fn formats_hours() {
        assert_eq!(format_hours(120), "2");
        assert_eq!(format_hours(90), "1.5");
        assert_eq!(format_hours(100), "1.67");
    }

    #[test]
    fn event_form_round_trips() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 6)
            .unwrap()
            .and_hms_opt(19, 0, 0)
            .unwrap();
        let mut e = Event::new("list", start);
        e.title = "Game night".into();
        e.neighborhood = "north-park".into();
        e.rrule = Repeat::Weekly.to_rrule(start);
        e.cancelled = vec![NaiveDate::from_ymd_opt(2026, 10, 13).unwrap()];
        e.categories = vec!["chill".into()];
        e.tags = vec!["beginner friendly".into(), "indoors".into()];
        let built = EventForm::new(e.clone(), false).build().unwrap();
        assert_eq!(built, e);
    }

    #[test]
    fn hidden_groups_are_not_offered_for_new_events() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 6)
            .unwrap()
            .and_hms_opt(19, 0, 0)
            .unwrap();
        let mut shown = Group::new("list");
        shown.name = "Shown".into();
        let mut hidden_group = Group::new("list");
        hidden_group.name = "Hidden".into();
        let groups = [shown, hidden_group.clone()];
        let hidden = HashSet::from([hidden_group.id.clone()]);
        let names = |f: &EventForm| -> Vec<String> {
            f.group_choices(&groups, &hidden)
                .into_iter()
                .map(|c| c.name)
                .collect()
        };

        let new = EventForm::new(Event::new("list", start), true);
        assert_eq!(names(&new)[1..], ["Shown"]);

        // An existing event in a hidden group still shows its group.
        let mut existing = Event::new("list", start);
        existing.group_id = Some(hidden_group.id.clone());
        assert_eq!(
            names(&EventForm::new(existing, false))[1..],
            ["Shown", "Hidden"]
        );
    }

    #[test]
    fn calendar_picks_into_the_date_field() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 6)
            .unwrap()
            .and_hms_opt(19, 0, 0)
            .unwrap();
        let mut f = EventForm::new(Event::new("list", start), true);
        f.update(Field::OpenCalendar, &[]);
        assert_eq!(f.calendar, NaiveDate::from_ymd_opt(2026, 10, 6));
        f.update(Field::Calendar(calendar::Action::Move(7)), &[]);
        f.update(Field::Calendar(calendar::Action::Month(1)), &[]);
        let cursor = f.calendar.unwrap();
        assert_eq!(cursor, NaiveDate::from_ymd_opt(2026, 11, 13).unwrap());
        f.update(Field::Calendar(calendar::Action::Pick(cursor)), &[]);
        assert_eq!(f.date, "11-13-2026");
        assert!(f.calendar.is_none());
    }

    #[test]
    fn existing_countywide_records_keep_it() {
        let start = NaiveDate::from_ymd_opt(2027, 8, 12)
            .unwrap()
            .and_hms_opt(22, 0, 0)
            .unwrap();
        let mut e = Event::new("list", start);
        e.title = "Perseids".into();
        e.neighborhood = "countywide".into();
        assert_eq!(EventForm::new(e.clone(), false).build().unwrap(), e);
    }

    #[test]
    fn event_form_rejects_bad_input() {
        let start = NaiveDate::from_ymd_opt(2026, 10, 6)
            .unwrap()
            .and_hms_opt(19, 0, 0)
            .unwrap();
        let mut f = EventForm::new(Event::new("list", start), true);
        assert!(f.build().is_err(), "title required");
        f.update(Field::Name("x".into()), &[]);
        f.update(Field::Repeat(Repeat::Custom), &[]);
        f.update(Field::Rule("FREQ=NOPE".into()), &[]);
        assert!(f.build().is_err(), "invalid rule");
        f.update(Field::Rule("FREQ=DAILY".into()), &[]);
        assert!(f.build().is_err(), "neighborhood required");
        let hillcrest = find_neighborhood("hillcrest").unwrap();
        f.update(Field::Neighborhood(hillcrest), &[]);
        assert!(f.build().is_ok());
    }
}
