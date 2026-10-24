use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{NaiveTime, TimeDelta};
use iced::widget::{button, checkbox, column, pick_list, row, rule, scrollable, space, text_input};
use iced::{Element, Fill, Task};
use sdsa_core::catalog::{NEIGHBORHOODS, Neighborhood, TAGS};
use sdsa_core::recurrence;
use sdsa_core::{Event, Group, Occurrence, Source, Store};

use crate::form::{self, Editor, Record};
use crate::{list, ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Events,
    Groups,
}

/// Neighborhood filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    All,
    One(Neighborhood),
}

impl Area {
    fn options() -> Vec<Area> {
        std::iter::once(Area::All)
            .chain(NEIGHBORHOODS.iter().copied().map(Area::One))
            .collect()
    }
}

impl std::fmt::Display for Area {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Area::All => f.write_str("All neighborhoods"),
            Area::One(n) => f.write_str(n.name),
        }
    }
}

/// How upcoming events are grouped under headings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Day,
    Week,
    Month,
}

impl Bucket {
    pub const ALL: [Bucket; 3] = [Bucket::Day, Bucket::Week, Bucket::Month];
}

impl std::fmt::Display for Bucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Bucket::Day => "By day",
            Bucket::Week => "By week",
            Bucket::Month => "By month",
        })
    }
}

/// How far ahead to list events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Horizon {
    Month,
    Quarter,
    Year,
}

impl Horizon {
    pub const ALL: [Horizon; 3] = [Horizon::Month, Horizon::Quarter, Horizon::Year];

    fn days(self) -> i64 {
        match self {
            Horizon::Month => 30,
            Horizon::Quarter => 90,
            Horizon::Year => 365,
        }
    }
}

impl std::fmt::Display for Horizon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Next {} days", self.days())
    }
}

#[derive(Debug, Default)]
pub struct Filters {
    pub search: String,
    pub area: Option<Neighborhood>,
    pub tags: HashSet<&'static str>,
    pub show_hidden: bool,
}

impl Filters {
    fn matches_text(&self, fields: &[&str]) -> bool {
        let needle = self.search.trim().to_lowercase();
        needle.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&needle))
    }

    fn matches_tags<'a>(&self, tags: impl IntoIterator<Item = &'a String>) -> bool {
        self.tags.is_empty() || tags.into_iter().any(|t| self.tags.contains(t.as_str()))
    }

    fn matches_area(&self, slug: &str) -> bool {
        self.area.is_none_or(|n| n.slug == slug)
    }

    pub fn group(&self, g: &Group, hidden: &HashSet<String>) -> bool {
        (self.show_hidden || !hidden.contains(&g.id))
            && self.matches_area(&g.neighborhood)
            && self.matches_tags(&g.tags)
            && self.matches_text(&[&g.name, &g.description, &g.address])
    }

    pub fn event(&self, e: &Event, group: Option<&Group>, hidden: &HashSet<String>) -> bool {
        let group_tags = group.into_iter().flat_map(|g| &g.tags);
        let group_name = group.map_or("", |g| g.name.as_str());
        (self.show_hidden || group.is_none_or(|g| !hidden.contains(&g.id)))
            && self.matches_area(&e.neighborhood)
            && self.matches_tags(e.tags.iter().chain(group_tags))
            && self.matches_text(&[&e.title, &e.description, &e.location, group_name])
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Tab(Tab),
    Search(String),
    Area(Area),
    Tag(&'static str, bool),
    ShowHidden(bool),
    ClearFilters,
    Bucket(Bucket),
    Horizon(Horizon),
    SetHidden(String, bool),
    OpenUrl(String),
    NewGroup,
    EditGroup(String),
    NewEvent(Option<String>),
    EditEvent(String),
    Form(form::Field),
    Save,
    Cancel,
    Delete,
    DismissStatus,
}

pub struct App {
    store: Store,
    defaults_path: PathBuf,
    user_path: PathBuf,
    pub groups: Vec<Group>,
    pub events: Vec<Event>,
    pub hidden: HashSet<String>,
    /// Upcoming occurrences of `events` within `horizon`, before filtering.
    pub occurrences: Vec<Occurrence>,
    pub filters: Filters,
    pub bucket: Bucket,
    pub horizon: Horizon,
    tab: Tab,
    editor: Option<Editor>,
    /// Message and whether it is an error.
    status: Option<(String, bool)>,
}

impl App {
    pub fn new(store: Store, defaults_path: PathBuf, user_path: PathBuf) -> Self {
        let status = store.defaults_warning.clone().map(|w| (w, true));
        let mut app = Self {
            store,
            defaults_path,
            user_path,
            groups: Vec::new(),
            events: Vec::new(),
            hidden: HashSet::new(),
            occurrences: Vec::new(),
            filters: Filters::default(),
            bucket: Bucket::Day,
            horizon: Horizon::Month,
            tab: Tab::Events,
            editor: None,
            status,
        };
        app.reload();
        app
    }

    pub fn title(&self) -> String {
        if self.store.is_admin() {
            "SD Social Activities — Admin".into()
        } else {
            "SD Social Activities".into()
        }
    }

    pub fn is_admin(&self) -> bool {
        self.store.is_admin()
    }

    pub fn can_edit(&self, source: Source) -> bool {
        self.store.can_edit(source)
    }

    pub fn group(&self, id: Option<&str>) -> Option<&Group> {
        id.and_then(|id| self.groups.iter().find(|g| g.id == id))
    }

    /// Resolves a group's logo, with relative paths taken from its database's folder.
    pub fn logo_path(&self, g: &Group) -> Option<PathBuf> {
        let logo = Path::new(g.logo_path.as_deref()?);
        let db = match g.source {
            Source::Defaults => &self.defaults_path,
            Source::User => &self.user_path,
        };
        let path = match db.parent() {
            Some(dir) if logo.is_relative() => dir.join(logo),
            _ => logo.to_path_buf(),
        };
        path.exists().then_some(path)
    }

    fn reload(&mut self) {
        let loaded = (|| -> sdsa_core::store::Result<_> {
            Ok((
                self.store.groups()?,
                self.store.events()?,
                self.store.hidden_groups()?,
            ))
        })();
        match loaded {
            Ok((groups, events, hidden)) => {
                self.groups = groups;
                self.events = events;
                self.hidden = hidden;
            }
            Err(e) => self.status = Some((format!("Couldn't load data: {e}"), true)),
        }
        self.expand();
    }

    fn expand(&mut self) {
        let from = recurrence::now_local().date().and_time(NaiveTime::MIN);
        let to = from + TimeDelta::days(self.horizon.days());
        self.occurrences = recurrence::occurrences(&self.events, from, to);
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tab(tab) => {
                self.tab = tab;
                self.editor = None;
            }
            Message::Search(s) => self.filters.search = s,
            Message::Area(Area::All) => self.filters.area = None,
            Message::Area(Area::One(n)) => self.filters.area = Some(n),
            Message::Tag(tag, on) => {
                if on {
                    self.filters.tags.insert(tag);
                } else {
                    self.filters.tags.remove(tag);
                }
            }
            Message::ShowHidden(on) => self.filters.show_hidden = on,
            Message::ClearFilters => self.filters = Filters::default(),
            Message::Bucket(b) => self.bucket = b,
            Message::Horizon(h) => {
                self.horizon = h;
                self.expand();
            }
            Message::SetHidden(id, hidden) => match self.store.set_hidden(&id, hidden) {
                Ok(()) => self.reload(),
                Err(e) => self.status = Some((e.to_string(), true)),
            },
            Message::OpenUrl(url) => {
                if let Err(e) = open::that_detached(&url) {
                    self.status = Some((format!("Couldn't open {url}: {e}"), true));
                }
            }
            Message::NewGroup => {
                let group = Group::new(self.store.new_record_source());
                self.editor = Some(Editor::Group(form::GroupForm::new(group, true)));
            }
            Message::EditGroup(id) => {
                if let Some(g) = self.group(Some(&id)).filter(|g| self.can_edit(g.source)) {
                    self.editor = Some(Editor::Group(form::GroupForm::new(g.clone(), false)));
                }
            }
            Message::NewEvent(group_id) => {
                let group = self.group(group_id.as_deref());
                // A user's event for one of their own groups stays with their data.
                let source = match group {
                    Some(g) if g.source == Source::User => Source::User,
                    _ => self.store.new_record_source(),
                };
                let start = recurrence::now_local()
                    .date()
                    .and_hms_opt(18, 0, 0)
                    .unwrap_or_default();
                let mut f = form::EventForm::new(Event::new(source, start), true);
                if let Some(g) = group {
                    f.choose_group(g);
                }
                self.editor = Some(Editor::Event(f));
            }
            Message::EditEvent(id) => {
                if let Some(e) = self
                    .events
                    .iter()
                    .find(|e| e.id == id && self.can_edit(e.source))
                {
                    self.editor = Some(Editor::Event(form::EventForm::new(e.clone(), false)));
                }
            }
            Message::Form(field) => {
                if let Some(editor) = &mut self.editor {
                    editor.update(field, &self.groups);
                }
            }
            Message::Save => self.save(),
            Message::Cancel => self.editor = None,
            Message::Delete => self.delete(),
            Message::DismissStatus => self.status = None,
        }
        Task::none()
    }

    fn save(&mut self) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let result = match editor.build() {
            Err(msg) => {
                editor.set_error(msg);
                return;
            }
            Ok(Record::Group(g)) => self.store.save_group(&g).map(|()| g.name),
            Ok(Record::Event(e)) => self.store.save_event(&e).map(|()| e.title),
        };
        match result {
            Ok(name) => {
                self.editor = None;
                self.status = Some((format!("Saved “{name}”"), false));
                self.reload();
            }
            Err(e) => editor.set_error(e.to_string()),
        }
    }

    fn delete(&mut self) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        // The first press asks for confirmation.
        if !editor.confirm_delete() {
            return;
        }
        let result = match editor.original() {
            Record::Group(g) => self.store.delete_group(&g).map(|()| g.name),
            Record::Event(e) => self.store.delete_event(&e).map(|()| e.title),
        };
        match result {
            Ok(name) => {
                self.editor = None;
                self.status = Some((format!("Deleted “{name}”"), false));
                self.reload();
            }
            Err(e) => editor.set_error(e.to_string()),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let main = match (&self.editor, self.tab) {
            (Some(editor), _) => editor.view(self),
            (None, Tab::Events) => list::events(self),
            (None, Tab::Groups) => list::groups(self),
        };

        let mut content = column![].spacing(12).padding(16).width(Fill);
        if let Some((msg, is_error)) = &self.status {
            content = content.push(
                row![
                    ui::banner(msg, *is_error),
                    button("Dismiss")
                        .style(button::text)
                        .on_press(Message::DismissStatus),
                ]
                .spacing(8)
                .align_y(iced::Center),
            );
        }

        row![self.sidebar(), rule::vertical(1), content.push(main)].into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let tab = |label, tab| {
            button(label)
                .width(Fill)
                .style(if self.tab == tab && self.editor.is_none() {
                    button::primary
                } else {
                    button::secondary
                })
                .on_press(Message::Tab(tab))
        };

        let mut header = column![ui::heading("SD Social")].spacing(4);
        if self.is_admin() {
            header = header.push(ui::chip("Admin: editing curated defaults"));
        }

        let tags = column(TAGS.iter().map(|t| {
            checkbox(self.filters.tags.contains(t.slug))
                .label(t.name)
                .on_toggle(move |on| Message::Tag(t.slug, on))
                .size(16)
                .text_size(14)
                .into()
        }))
        .spacing(6);

        let area = self.filters.area.map_or(Area::All, Area::One);

        let sidebar = column![
            header,
            tab("Upcoming events", Tab::Events),
            tab("Groups", Tab::Groups),
            row![
                button("+ Group")
                    .on_press(Message::NewGroup)
                    .style(button::subtle),
                button("+ Event")
                    .on_press(Message::NewEvent(None))
                    .style(button::subtle),
            ]
            .spacing(8),
            rule::horizontal(1),
            ui::bold("Filters"),
            text_input("Search…", &self.filters.search).on_input(Message::Search),
            pick_list(Area::options(), Some(area), Message::Area).width(Fill),
            ui::muted("Categories"),
            tags,
            checkbox(self.filters.show_hidden)
                .label("Show hidden groups")
                .on_toggle(Message::ShowHidden)
                .size(16)
                .text_size(14),
            button("Clear filters")
                .style(button::text)
                .on_press(Message::ClearFilters),
            space::vertical(),
        ]
        .spacing(10)
        .padding(16)
        .width(240);

        scrollable(sidebar).height(Fill).into()
    }
}
