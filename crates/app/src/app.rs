use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Duration;

use chrono::{NaiveTime, TimeDelta};
use iced::time::Instant;
use iced::widget::operation;
use iced::widget::{button, column, row, rule, scrollable, space, text_input};
use iced::{Animation, Element, Fill, Subscription, Task, keyboard, window};
use sdsa_core::catalog::CATEGORIES;
use sdsa_core::recurrence;
use sdsa_core::store::Installed;
use sdsa_core::{Event, Group, IndexEntry, ListFile, ListIndex, ListInfo, Occurrence, Store};

use crate::form::{self, Editor, Record};
use crate::lists::{self, Lists, UpdateState};
use crate::location::{self, Area};
use crate::{calendar, focus, focusable};
use crate::{list, saved, search, ui};

/// The scroll area of whichever panel is showing on the right.
pub const MAIN_SCROLL: &str = "main-scroll";
/// The sidebar's search box, for Ctrl+S.
pub const SEARCH_INPUT: &str = "search";
/// The first button of each panel on the right, for Ctrl+R.
pub const RESULTS_START: &str = "results-start";

/// Setting saying the lists catalog has been read before, so lists that
/// appear in it later start off.
const INDEX_READ: &str = "index_read";

/// How long a tag takes to fade to or from gray.
const TAG_FADE: Duration = Duration::from_millis(250);

/// How far the arrow keys and Page Up/Down scroll the panel on the right.
const LINE_SCROLL: f32 = 48.0;
const PAGE_SCROLL: f32 = 400.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Events,
    Groups,
    Lists,
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
    TenDays,
    Month,
    Quarter,
    Year,
}

impl Horizon {
    pub const ALL: [Horizon; 4] = [
        Horizon::TenDays,
        Horizon::Month,
        Horizon::Quarter,
        Horizon::Year,
    ];

    fn days(self) -> i64 {
        match self {
            Horizon::TenDays => 10,
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
    pub area: Area,
    pub categories: HashSet<&'static str>,
    /// Free-form tags; a record must have all of them.
    pub tags: BTreeSet<String>,
}

impl Filters {
    /// Whether anything narrows the results.
    pub fn is_active(&self) -> bool {
        !self.search.trim().is_empty()
            || self.area != Area::All
            || !self.categories.is_empty()
            || !self.tags.is_empty()
    }

    fn matches_text(&self, fields: &[&str]) -> bool {
        self.search.trim().is_empty() || fields.iter().any(|f| search::matches(f, &self.search))
    }

    /// Any of the selected categories.
    fn matches_categories<'a>(&self, categories: impl IntoIterator<Item = &'a String>) -> bool {
        self.categories.is_empty()
            || categories
                .into_iter()
                .any(|c| self.categories.contains(c.as_str()))
    }

    /// All of the selected tags.
    fn matches_tags<'a>(&self, tags: impl IntoIterator<Item = &'a String> + Clone) -> bool {
        self.tags
            .iter()
            .all(|want| tags.clone().into_iter().any(|t| t == want))
    }

    /// Whether `g` matches, among hidden groups if `want_hidden`, otherwise
    /// among visible ones.
    pub fn group(&self, g: &Group, hidden: &HashSet<String>, want_hidden: bool) -> bool {
        hidden.contains(&g.id) == want_hidden
            && self.area.contains(&g.neighborhood)
            && self.matches_categories(&g.categories)
            && self.matches_tags(&g.tags)
            && self.matches_text(&[&g.name, &g.description, &g.address])
    }

    pub fn event(&self, e: &Event, group: Option<&Group>, hidden: &HashSet<String>) -> bool {
        let group_categories = group.into_iter().flat_map(|g| &g.categories);
        let group_tags = group.into_iter().flat_map(|g| &g.tags);
        let group_name = group.map_or("", |g| g.name.as_str());
        group.is_none_or(|g| !hidden.contains(&g.id))
            && self.area.contains(&e.neighborhood)
            && self.matches_categories(e.categories.iter().chain(group_categories))
            && self.matches_tags(e.tags.iter().chain(group_tags))
            && self.matches_text(&[&e.title, &e.description, &e.location, group_name])
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Tab(Tab),
    Search(String),
    Area(Area),
    /// Opens a location dropdown, or closes the open one.
    Dropdown(Option<location::Dropdown>),
    /// An entry was clicked in an open dropdown: do it, then close the list.
    Picked(Box<Message>),
    /// Picking a region heading in an editor, where only places are allowed.
    Noop,
    Category(&'static str, bool),
    ToggleTag(String),
    /// Switches the Groups panel between visible and hidden groups.
    ViewHidden(bool),
    ClearFilters,
    Bucket(Bucket),
    Horizon(Horizon),
    SetHidden(String, bool),
    OpenUrl(String),
    /// Saves an event as a calendar file, asking where first.
    AddToCalendar(String),
    CalendarSaved(String, Option<std::path::PathBuf>),
    NewGroup,
    EditGroup(String),
    NewEvent(Option<String>),
    EditEvent(String),
    Form(form::Field),
    Lists(lists::Msg),
    Save,
    Cancel,
    Delete,
    DismissStatus,
    /// A key press no widget used, for shortcuts and Tab.
    Key(keyboard::Key, keyboard::Modifiers),
    /// Scrolls the panel on the right by this much (negative is up).
    Scroll(f32),
    /// A frame is being drawn while tags fade.
    Frame,
    /// The catalog of published lists, downloaded on launch.
    IndexFetched(Result<String, String>),
    /// The catalog's lists that aren't installed yet, downloaded. `first` is
    /// whether this is the first time the catalog was read.
    IndexListsFetched {
        first: bool,
        fetched: Vec<(IndexEntry, Result<String, String>)>,
    },
}

pub struct App {
    store: Store,
    pub list_infos: Vec<ListInfo>,
    pub lists: Lists,
    pub groups: Vec<Group>,
    pub events: Vec<Event>,
    pub hidden: HashSet<String>,
    /// Lists turned off in the Lists panel; their groups and events aren't shown.
    pub lists_off: HashSet<String>,
    /// Every free-form tag in use, sorted.
    pub tags: Vec<String>,
    /// Upcoming occurrences of `events` within `horizon`, before filtering.
    pub occurrences: Vec<Occurrence>,
    pub filters: Filters,
    pub bucket: Bucket,
    pub horizon: Horizon,
    /// The Groups panel is listing hidden groups instead of visible ones.
    pub viewing_hidden: bool,
    /// The filters, grouping and range as last saved, to save again only on a change.
    saved_view: String,
    /// The location dropdown that's open, if any.
    open_dropdown: Option<location::Dropdown>,
    /// Each filter tag's fade between grayed out (false) and not (true).
    tag_fades: HashMap<String, Animation<bool>>,
    /// The time the view is drawn at, for the fades.
    now: Instant,
    tab: Tab,
    editor: Option<Editor>,
    /// Message and whether it is an error.
    status: Option<(String, bool)>,
}

impl App {
    pub fn new(store: Store, status: Option<(String, bool)>) -> Self {
        let mut app = Self {
            store,
            list_infos: Vec::new(),
            lists: Lists::default(),
            groups: Vec::new(),
            events: Vec::new(),
            hidden: HashSet::new(),
            lists_off: HashSet::new(),
            tags: Vec::new(),
            occurrences: Vec::new(),
            filters: Filters::default(),
            bucket: Bucket::Day,
            horizon: Horizon::Month,
            viewing_hidden: false,
            saved_view: String::new(),
            open_dropdown: None,
            tag_fades: HashMap::new(),
            now: Instant::now(),
            tab: Tab::Events,
            editor: None,
            status,
        };
        // Restored before loading, which drops tags no longer in use.
        if let Ok(Some(view)) = app.store.setting(saved::KEY) {
            saved::decode(&view, &mut app.filters, &mut app.bucket, &mut app.horizon);
            app.saved_view = view;
        }
        app.reload();
        app.fade_tags();
        app
    }

    /// Starts tags fading to or from gray where whether they have results
    /// changed. Tags seen for the first time start where they belong.
    fn fade_tags(&mut self) {
        self.now = Instant::now();
        let in_results: HashSet<String> = self
            .tags_in_results()
            .into_iter()
            .map(String::from)
            .collect();
        self.tag_fades.retain(|t, _| self.tags.contains(t));
        for t in &self.tags {
            let has = in_results.contains(t);
            match self.tag_fades.get_mut(t) {
                Some(fade) if fade.value() != has => fade.go_mut(has, self.now),
                Some(_) => {}
                None => {
                    let fade = Animation::new(has).duration(TAG_FADE);
                    self.tag_fades.insert(t.clone(), fade);
                }
            }
        }
    }

    /// How far tag `t` is from grayed out (0) to not (1), right now.
    pub fn tag_presence(&self, t: &str) -> f32 {
        self.tag_fades
            .get(t)
            .map_or(1.0, |fade| fade.interpolate(0.0, 1.0, self.now))
    }

    /// The app as it starts: `new`, then a check of the list catalog for
    /// lists to add.
    pub fn boot(store: Store, status: Option<(String, bool)>) -> (Self, Task<Message>) {
        let fetch = Task::perform(
            lists::in_background(|| lists::read_location(lists::INDEX_URL)),
            Message::IndexFetched,
        );
        (Self::new(store, status), fetch)
    }

    pub fn title(&self) -> String {
        "Social Activities!".into()
    }

    /// Whether the form's neighborhood dropdown is open.
    pub fn place_open(&self) -> bool {
        self.open_dropdown == Some(location::Dropdown::Place)
    }

    /// Whether records in `list_id` can be edited: only lists you own.
    pub fn can_edit(&self, list_id: &str) -> bool {
        self.list_infos.iter().any(|l| l.id == list_id && l.owned)
    }

    /// Lists you own, which new and edited records can be saved into.
    pub fn owned_lists(&self) -> impl Iterator<Item = &ListInfo> {
        self.list_infos.iter().filter(|l| l.owned)
    }

    /// Whether `list_id`'s groups and events are shown.
    pub fn is_on(&self, list_id: &str) -> bool {
        !self.lists_off.contains(list_id)
    }

    /// Groups that can't be chosen for an event: hidden ones, and those in
    /// lists turned off.
    pub fn not_offered(&self) -> HashSet<String> {
        let off = self.groups.iter().filter(|g| !self.is_on(&g.list_id));
        let off = off.map(|g| g.id.clone());
        self.hidden.iter().cloned().chain(off).collect()
    }

    /// Hidden groups that still exist, in lists that are on; the hidden list
    /// can outlive a group removed from the curated defaults.
    pub fn hidden_count(&self) -> usize {
        self.groups
            .iter()
            .filter(|g| self.hidden.contains(&g.id) && self.is_on(&g.list_id))
            .count()
    }

    /// Tags on the results the panel is showing, given every filter. Adding
    /// one of these to the selected tags still leaves something to show; any
    /// other tag would leave nothing. On the Lists panel, where filters don't
    /// apply, it's the Events panel's results.
    pub fn tags_in_results(&self) -> HashSet<&str> {
        let mut tags = HashSet::new();
        if self.tab == Tab::Groups {
            let groups = (self.groups.iter())
                .filter(|g| self.is_on(&g.list_id))
                .filter(|g| self.filters.group(g, &self.hidden, self.viewing_hidden));
            tags.extend(groups.flat_map(|g| &g.tags).map(String::as_str));
        } else {
            let mut seen = HashSet::new();
            for occ in &self.occurrences {
                if !seen.insert(occ.event) {
                    continue;
                }
                let event = &self.events[occ.event];
                let group = self.group(event.group_id.as_deref());
                if self.filters.event(event, group, &self.hidden) {
                    let group_tags = group.into_iter().flat_map(|g| &g.tags);
                    tags.extend(event.tags.iter().chain(group_tags).map(String::as_str));
                }
            }
        }
        tags
    }

    pub fn group(&self, id: Option<&str>) -> Option<&Group> {
        id.and_then(|id| self.groups.iter().find(|g| g.id == id))
    }

    /// The list new records go into: the one given if it's yours, else the one
    /// you last saved to, else your first list (created if you have none).
    fn target_list(&mut self, preferred: Option<&str>) -> Option<String> {
        if let Some(id) = preferred.filter(|id| self.can_edit(id)) {
            return Some(id.to_string());
        }
        // Otherwise the list you last saved to, so curating one list doesn't
        // mean choosing it every time.
        if let Ok(Some(id)) = self.store.last_list() {
            return Some(id);
        }
        match self.store.default_list() {
            Ok(list) => {
                if !self.can_edit(&list.id) {
                    self.reload();
                }
                Some(list.id)
            }
            Err(e) => {
                self.error(e);
                None
            }
        }
    }

    fn error(&mut self, e: impl std::fmt::Display) {
        self.status = Some((e.to_string(), true));
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), false));
    }

    fn reload(&mut self) {
        let loaded = (|| -> sdsa_core::store::Result<_> {
            Ok((
                self.store.lists()?,
                self.store.groups()?,
                self.store.events()?,
                self.store.hidden_groups()?,
                self.store.lists_off()?,
            ))
        })();
        match loaded {
            Ok((lists, groups, events, hidden, lists_off)) => {
                self.list_infos = lists;
                self.groups = groups;
                self.events = events;
                self.hidden = hidden;
                self.lists_off = lists_off;
                // Leave the hidden view once the last group is unhidden.
                if self.hidden_count() == 0 {
                    self.viewing_hidden = false;
                }
                let groups = self.groups.iter().filter(|g| self.is_on(&g.list_id));
                let events = self.events.iter().filter(|e| self.is_on(&e.list_id));
                let in_use: BTreeSet<&String> = (groups.flat_map(|g| &g.tags))
                    .chain(events.flat_map(|e| &e.tags))
                    .collect();
                self.tags = in_use.into_iter().cloned().collect();
                // Drop selections for tags that no longer exist.
                self.filters.tags.retain(|t| self.tags.contains(t));
            }
            Err(e) => self.status = Some((format!("Couldn't load data: {e}"), true)),
        }
        self.expand();
    }

    /// Saves the filters, grouping and range if they changed.
    fn save_view(&mut self) {
        let view = saved::encode(&self.filters, self.bucket, self.horizon);
        if view != self.saved_view {
            match self.store.set_setting(saved::KEY, &view) {
                Ok(()) => self.saved_view = view,
                Err(e) => self.error(format!("Couldn't save your filters: {e}")),
            }
        }
    }

    fn expand(&mut self) {
        let from = recurrence::now_local().date().and_time(NaiveTime::MIN);
        let to = from + TimeDelta::days(self.horizon.days());
        self.occurrences = recurrence::occurrences(&self.events, from, to);
        let events = &self.events;
        let lists_off = &self.lists_off;
        self.occurrences
            .retain(|o| !lists_off.contains(&events[o.event].list_id));
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let was_editing = self.editor.is_some();
        let tab = self.tab;
        let task = self.handle(message);
        self.save_view();
        self.fade_tags();
        // A dropdown doesn't stay open across a change of panel or form.
        if self.tab != tab || self.editor.is_some() != was_editing {
            self.open_dropdown = None;
        }
        // A newly opened form starts with its first field focused, so typing
        // and Tab work straight away.
        if !was_editing && self.editor.is_some() {
            return Task::batch([task, operation::focus(form::FIRST_INPUT)]);
        }
        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tab(tab) => {
                self.tab = tab;
                self.editor = None;
                self.viewing_hidden = false;
            }
            Message::ViewHidden(on) => self.viewing_hidden = on,
            Message::Search(s) => self.filters.search = s,
            Message::Area(area) => self.filters.area = area,
            Message::Dropdown(which) => self.open_dropdown = which,
            Message::Picked(message) => {
                self.open_dropdown = None;
                return self.handle(*message);
            }
            Message::Noop => {}
            Message::Category(slug, on) => {
                if on {
                    self.filters.categories.insert(slug);
                } else {
                    self.filters.categories.remove(slug);
                }
            }
            Message::ToggleTag(tag) => {
                if !self.filters.tags.remove(&tag) {
                    self.filters.tags.insert(tag);
                }
            }
            Message::ClearFilters => self.filters = Filters::default(),
            Message::Bucket(b) => self.bucket = b,
            Message::Horizon(h) => {
                self.horizon = h;
                self.expand();
            }
            Message::SetHidden(id, hidden) => match self.store.set_hidden(&id, hidden) {
                Ok(()) => self.reload(),
                Err(e) => self.error(e),
            },
            Message::OpenUrl(url) => {
                if let Err(e) = open::that_detached(&url) {
                    self.error(format!("Couldn't open {url}: {e}"));
                }
            }
            Message::AddToCalendar(id) => {
                if let Some(event) = self.events.iter().find(|e| e.id == id) {
                    let name = format!("{}.ics", lists::slug_or(&event.title, "event"));
                    return Task::perform(
                        lists::in_background(move || lists::save_calendar(name)),
                        move |path| Message::CalendarSaved(id.clone(), path),
                    );
                }
            }
            Message::CalendarSaved(id, Some(path)) => {
                if let Some(event) = self.events.iter().find(|e| e.id == id) {
                    let group = self.group(event.group_id.as_deref());
                    let ics = sdsa_core::ics::calendar(event, group, chrono::Utc::now());
                    match std::fs::write(&path, ics) {
                        Ok(()) => self.info(format!(
                            "Saved {}. Open it to add “{}” to your calendar.",
                            path.display(),
                            event.title
                        )),
                        Err(e) => self.error(format!("Couldn't save {}: {e}", path.display())),
                    }
                }
            }
            Message::CalendarSaved(_, None) => {}
            Message::NewGroup => {
                if let Some(list) = self.target_list(None) {
                    let group = Group::new(&list);
                    self.editor = Some(Editor::Group(form::GroupForm::new(group, true)));
                }
            }
            Message::EditGroup(id) => {
                if let Some(g) = self.group(Some(&id)).filter(|g| self.can_edit(&g.list_id)) {
                    self.editor = Some(Editor::Group(form::GroupForm::new(g.clone(), false)));
                }
            }
            Message::NewEvent(group_id) => {
                let group = self
                    .group(group_id.as_deref())
                    .filter(|g| !self.hidden.contains(&g.id))
                    .cloned();
                // Events for a group in one of your lists go in that list.
                let Some(list) = self.target_list(group.as_ref().map(|g| g.list_id.as_str()))
                else {
                    return Task::none();
                };
                let start = recurrence::now_local()
                    .date()
                    .and_hms_opt(18, 0, 0)
                    .unwrap_or_default();
                let mut f = form::EventForm::new(Event::new(&list, start), true);
                if let Some(g) = &group {
                    f.choose_group(g, &self.list_infos);
                }
                self.editor = Some(Editor::Event(f));
            }
            Message::EditEvent(id) => {
                if let Some(e) = self
                    .events
                    .iter()
                    .find(|e| e.id == id && self.can_edit(&e.list_id))
                {
                    self.editor = Some(Editor::Event(form::EventForm::new(e.clone(), false)));
                }
            }
            Message::Form(field) => {
                // Opening the calendar takes focus from the text fields so the
                // arrow keys reach it; closing it returns focus to the date.
                let focus = match &field {
                    form::Field::OpenCalendar => Some(calendar::FOCUS_NOTHING),
                    form::Field::Calendar(calendar::Action::Pick(_) | calendar::Action::Close) => {
                        Some(form::DATE_INPUT)
                    }
                    _ => None,
                };
                if let Some(editor) = &mut self.editor {
                    editor.update(field, &self.groups, &self.list_infos);
                }
                if let Some(id) = focus {
                    return operation::focus(id);
                }
            }
            Message::Lists(msg) => return self.update_lists(msg),
            Message::Save => self.save(),
            Message::Cancel => self.editor = None,
            Message::Delete => self.delete(),
            Message::DismissStatus => self.status = None,
            Message::Key(key, modifiers) => return self.on_key(&key, modifiers),
            // The time is taken in `update`, after every message.
            Message::Frame => {}
            Message::IndexFetched(result) => return self.index_fetched(result),
            Message::IndexListsFetched { first, fetched } => {
                self.install_index_lists(first, fetched)
            }
            Message::Scroll(dy) => {
                return operation::scroll_by(
                    MAIN_SCROLL,
                    scrollable::AbsoluteOffset { x: 0.0, y: dy },
                );
            }
        }
        Task::none()
    }

    /// Downloads the catalog's lists that should be added. Failing to reach
    /// the catalog isn't worth interrupting anyone for; it's tried again on
    /// the next launch. The first time it's read counts as the first launch,
    /// even if an earlier launch was offline.
    fn index_fetched(&mut self, result: Result<String, String>) -> Task<Message> {
        let index = result.and_then(|json| ListIndex::parse(&json).map_err(|e| e.to_string()));
        let index = match index {
            Ok(index) => index,
            Err(e) => {
                eprintln!("list catalog: {e}");
                return Task::none();
            }
        };
        let first = matches!(self.store.setting(INDEX_READ), Ok(None));
        if first && let Err(e) = self.store.set_setting(INDEX_READ, "1") {
            eprintln!("list catalog: {e}");
        }
        let wanted: Vec<IndexEntry> = (index.lists.into_iter())
            .filter(|e| self.store.wants(&e.id).unwrap_or(false))
            .collect();
        if wanted.is_empty() {
            return Task::none();
        }
        Task::perform(
            lists::in_background(move || {
                (wanted.into_iter())
                    .map(|e| {
                        let json = lists::read_location(&e.url);
                        (e, json)
                    })
                    .collect()
            }),
            move |fetched| Message::IndexListsFetched { first, fetched },
        )
    }

    /// Adds the catalog's new lists. On the first launch they start on or off
    /// as the catalog says; lists that appear in it later start off, so they
    /// don't change what someone's used to seeing without asking.
    fn install_index_lists(
        &mut self,
        first: bool,
        fetched: Vec<(IndexEntry, Result<String, String>)>,
    ) {
        let (mut on, mut off, mut failed) = (Vec::new(), Vec::new(), Vec::new());
        for (entry, json) in fetched {
            let start_on = first && entry.default_on;
            let installed = json.and_then(|json| {
                let file = ListFile::parse(&json).map_err(|e| e.to_string())?;
                (self.store)
                    .install_from_index(&file, &entry.url, start_on)
                    .map(|installed| (file.name, installed))
                    .map_err(|e| e.to_string())
            });
            match installed {
                Ok((name, Installed::Added)) if start_on => on.push(name),
                Ok((name, Installed::Added)) => off.push(name),
                Ok(_) => {}
                Err(e) => failed.push(format!("Couldn't add “{}”: {e}", entry.name)),
            }
        }
        if on.is_empty() && off.is_empty() && failed.is_empty() {
            return;
        }
        self.reload();
        let quoted = |names: &[String]| {
            let names: Vec<String> = names.iter().map(|n| format!("“{n}”")).collect();
            names.join(", ")
        };
        let mut parts = Vec::new();
        if !on.is_empty() {
            parts.push(format!("Added {} from the lists catalog.", quoted(&on)));
        }
        if !off.is_empty() && first {
            parts.push(format!(
                "Also added {}, turned off for now; turn them on in Lists.",
                quoted(&off)
            ));
        } else if !off.is_empty() {
            let (lists, they) = match off.len() {
                1 => ("A new list was", "It's"),
                _ => ("New lists were", "They're"),
            };
            parts.push(format!(
                "{lists} added to the app: {}. {they} off for now; turn {} on in Lists.",
                quoted(&off),
                if off.len() == 1 { "it" } else { "them" },
            ));
        }
        parts.extend(failed.iter().cloned());
        self.status = Some((parts.join(" "), !failed.is_empty()));
    }

    fn list_name(&self, id: &str) -> String {
        self.list_infos
            .iter()
            .find(|l| l.id == id)
            .map_or_else(String::new, |l| l.name.clone())
    }

    fn update_lists(&mut self, msg: lists::Msg) -> Task<Message> {
        use lists::Msg;
        let wrap = Message::Lists;
        match msg {
            Msg::SetOn(id, on) => match self.store.set_list_on(&id, on) {
                Ok(()) => self.reload(),
                Err(e) => self.error(e),
            },
            Msg::NewList => match self.store.create_list("New List") {
                Ok(list) => {
                    self.reload();
                    self.lists.editing = Some(lists::ListEdit::of(&list));
                }
                Err(e) => self.error(e),
            },
            Msg::Edit(id) => {
                self.lists.editing = self
                    .list_infos
                    .iter()
                    .find(|l| l.id == id)
                    .map(lists::ListEdit::of);
            }
            Msg::EditName(s) => {
                if let Some(edit) = &mut self.lists.editing {
                    edit.name = s;
                }
            }
            Msg::EditDescription(s) => {
                if let Some(edit) = &mut self.lists.editing {
                    edit.description = s;
                }
            }
            Msg::EditUpdateUrl(s) => {
                if let Some(edit) = &mut self.lists.editing {
                    edit.update_url = s;
                }
            }
            Msg::SaveEdit => {
                let Some(edit) = self.lists.editing.take() else {
                    return Task::none();
                };
                if let Err(msg) = edit.validate() {
                    self.lists.editing = Some(edit);
                    self.error(msg);
                    return Task::none();
                }
                let saved = self.store.edit_list(
                    &edit.id,
                    &edit.name,
                    &edit.description,
                    Some(&edit.update_url),
                );
                match saved {
                    Ok(()) => self.reload(),
                    Err(e) => {
                        self.lists.editing = Some(edit);
                        self.error(e);
                    }
                }
            }
            Msg::CancelEdit => self.lists.editing = None,
            Msg::Export(id) => {
                let name = lists::file_name(&self.list_name(&id));
                return Task::perform(
                    lists::in_background(move || lists::save_file(name)),
                    move |path| wrap(Msg::Exported(id.clone(), path)),
                );
            }
            Msg::Exported(id, Some(path)) => {
                let written = self
                    .store
                    .export(&id)
                    .map_err(|e| e.to_string())
                    .and_then(|file| {
                        std::fs::write(&path, file.to_json() + "\n").map_err(|e| e.to_string())
                    });
                match written {
                    Ok(()) => self.info(format!(
                        "Exported “{}” to {}",
                        self.list_name(&id),
                        path.display()
                    )),
                    Err(e) => self.error(format!("Couldn't export: {e}")),
                }
            }
            Msg::Exported(_, None) => {}
            Msg::Location(s) => self.lists.location = s,
            Msg::Maintain(on) => self.lists.maintain = on,
            Msg::Browse => {
                return Task::perform(lists::in_background(lists::pick_file), move |path| {
                    wrap(Msg::Browsed(path))
                });
            }
            Msg::Browsed(Some(path)) => self.lists.location = path.display().to_string(),
            Msg::Browsed(None) => {}
            Msg::Add => {
                let location = self.lists.location.trim().to_string();
                if location.is_empty() || self.lists.adding {
                    return Task::none();
                }
                // Remember local files by absolute path, so later checks find them.
                let location = if lists::is_url(&location) {
                    location
                } else {
                    std::fs::canonicalize(&location).map_or(location, |p| p.display().to_string())
                };
                self.lists.adding = true;
                let fetching = location.clone();
                return Task::perform(
                    lists::in_background(move || lists::read_location(&fetching)),
                    move |result| {
                        wrap(Msg::Fetched {
                            location: location.clone(),
                            result,
                        })
                    },
                );
            }
            Msg::Fetched { location, result } => {
                self.lists.adding = false;
                let added = result.and_then(|json| {
                    let file = ListFile::parse(&json).map_err(|e| e.to_string())?;
                    let installed = self
                        .store
                        .add_list(&file, Some(location), self.lists.maintain)
                        .map_err(|e| e.to_string())?;
                    Ok((file, installed))
                });
                match added {
                    Ok((file, installed)) => {
                        self.lists.location.clear();
                        self.lists.maintain = false;
                        self.lists.updates.remove(&file.id);
                        self.reload();
                        self.info(match installed {
                            Installed::Replaced => format!("Replaced “{}”", file.name),
                            _ => format!("Added “{}”", file.name),
                        });
                    }
                    Err(e) => self.error(e),
                }
            }
            Msg::Remove(id) => {
                if self.lists.confirm_remove.as_deref() != Some(&id) {
                    self.lists.confirm_remove = Some(id);
                    return Task::none();
                }
                self.lists.confirm_remove = None;
                let name = self.list_name(&id);
                match self.store.remove_list(&id) {
                    Ok(()) => {
                        self.lists.updates.remove(&id);
                        self.reload();
                        self.info(format!("Removed “{name}”"));
                    }
                    Err(e) => self.error(e),
                }
            }
            Msg::CancelRemove => self.lists.confirm_remove = None,
            Msg::Check(id) => return self.check_for_update(id),
            Msg::CheckAll => {
                let ids: Vec<String> = self
                    .list_infos
                    .iter()
                    .filter(|l| !l.owned && l.update_location().is_some())
                    .map(|l| l.id.clone())
                    .collect();
                return Task::batch(ids.into_iter().map(|id| self.check_for_update(id)));
            }
            Msg::Checked { id, result } => {
                let state = result
                    .and_then(|json| ListFile::parse(&json).map_err(|e| e.to_string()))
                    .and_then(|file| {
                        if file.id != id {
                            return Err(format!(
                                "That location now has a different list, “{}”.",
                                file.name
                            ));
                        }
                        match self.store.is_update(&file) {
                            Ok(true) => Ok(UpdateState::Available(Box::new(file))),
                            Ok(false) => Ok(UpdateState::UpToDate),
                            Err(e) => Err(e.to_string()),
                        }
                    })
                    .unwrap_or_else(UpdateState::Failed);
                self.lists.updates.insert(id, state);
            }
            Msg::Update(id) => {
                if let Some(UpdateState::Available(file)) = self.lists.updates.remove(&id) {
                    match self.store.update_list(&file) {
                        Ok(_) => {
                            self.reload();
                            self.info(format!("Updated “{}”", file.name));
                        }
                        Err(e) => self.error(e),
                    }
                }
            }
        }
        Task::none()
    }

    fn check_for_update(&mut self, id: String) -> Task<Message> {
        let Some(location) = self
            .list_infos
            .iter()
            .find(|l| l.id == id)
            .and_then(|l| l.update_location())
            .map(str::to_string)
        else {
            return Task::none();
        };
        self.lists.updates.insert(id.clone(), UpdateState::Checking);
        Task::perform(
            lists::in_background(move || lists::read_location(&location)),
            move |result| {
                Message::Lists(lists::Msg::Checked {
                    id: id.clone(),
                    result,
                })
            },
        )
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
            Ok(Record::Group(g)) => self.store.save_group(&g).map(|()| (g.name, g.list_id)),
            Ok(Record::Event(e)) => self.store.save_event(&e).map(|()| (e.title, e.list_id)),
        };
        match result {
            Ok((name, list_id)) => {
                self.editor = None;
                let mut msg = format!("Saved “{name}”");
                if !self.is_on(&list_id) {
                    msg += &format!(
                        " to “{}”, which is turned off. Turn it on in Lists to see it.",
                        self.list_name(&list_id)
                    );
                }
                self.status = Some((msg, false));
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

    pub fn subscription(&self) -> Subscription<Message> {
        let keys = keyboard::listen().filter_map(|event| match event {
            keyboard::Event::KeyPressed { key, modifiers, .. } => {
                Some(Message::Key(key, modifiers))
            }
            _ => None,
        });
        // Redraw every frame only while something is fading.
        let fading = (self.tag_fades.values()).any(|fade| fade.is_animating(self.now));
        if fading {
            Subscription::batch([keys, window::frames().map(|_| Message::Frame)])
        } else {
            keys
        }
    }

    /// Keyboard shortcuts: Ctrl+E events, Ctrl+G groups, Ctrl+L lists, Ctrl+N
    /// new (event, group or list, by panel), Ctrl+S search, Ctrl+R the panel on
    /// the right; Tab and Shift+Tab move between fields; arrows, Page Up/Down,
    /// Home and End scroll; Down in the date field opens its calendar, which
    /// then takes the arrow keys.
    fn on_key(&mut self, key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Task<Message> {
        if let Some(action) = self.editor.as_ref().and_then(|e| e.calendar_action(key)) {
            return self.update(Message::Form(form::Field::Calendar(action)));
        }
        // Down in the event's date field opens the calendar.
        if let keyboard::Key::Named(keyboard::key::Named::ArrowDown) = key.as_ref()
            && matches!(self.editor, Some(Editor::Event(_)))
        {
            return operation::is_focused(form::DATE_INPUT).map(|focused| {
                if focused {
                    Message::Form(form::Field::OpenCalendar)
                } else {
                    Message::Scroll(LINE_SCROLL)
                }
            });
        }
        if let keyboard::Key::Named(keyboard::key::Named::Tab) = key.as_ref() {
            return focus::step(!modifiers.shift());
        }
        // Scrolling the panel on the right. Text fields keep Home and End.
        if modifiers.is_empty()
            && let keyboard::Key::Named(named) = key.as_ref()
        {
            use keyboard::key::Named;
            let scroll = |dy| {
                operation::scroll_by(MAIN_SCROLL, scrollable::AbsoluteOffset { x: 0.0, y: dy })
            };
            match named {
                Named::ArrowUp => return scroll(-LINE_SCROLL),
                Named::ArrowDown => return scroll(LINE_SCROLL),
                Named::PageUp => return scroll(-PAGE_SCROLL),
                Named::PageDown => return scroll(PAGE_SCROLL),
                Named::Home => {
                    return operation::snap_to(MAIN_SCROLL, scrollable::RelativeOffset::START);
                }
                Named::End => return operation::snap_to_end(MAIN_SCROLL),
                _ => {}
            }
        }
        if !modifiers.command() {
            return Task::none();
        }
        let keyboard::Key::Character(c) = key.as_ref() else {
            return Task::none();
        };
        match c.to_lowercase().as_str() {
            "e" => self.update(Message::Tab(Tab::Events)),
            "g" => self.update(Message::Tab(Tab::Groups)),
            "l" => self.update(Message::Tab(Tab::Lists)),
            // Into the search box, its text selected so typing replaces it.
            "s" => operation::focus(SEARCH_INPUT).chain(operation::select_all(SEARCH_INPUT)),
            // Into the panel on the right: a form's first field, or the panel's
            // first button, so Tab continues there and arrows scroll it.
            "r" if self.editor.is_some() => {
                operation::focus(form::FIRST_INPUT).chain(operation::select_all(form::FIRST_INPUT))
            }
            "r" => operation::focus(RESULTS_START),
            // Starting something new from a form would throw away its edits.
            "n" if self.editor.is_none() => match self.tab {
                Tab::Events => self.update(Message::NewEvent(None)),
                Tab::Groups => self.update(Message::NewGroup),
                Tab::Lists => self.update(Message::Lists(lists::Msg::NewList)),
            },
            _ => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let main = match (&self.editor, self.tab) {
            (Some(editor), _) => editor.view(self),
            (None, Tab::Events) => list::events(self),
            (None, Tab::Groups) => list::groups(self),
            (None, Tab::Lists) => lists::view(self),
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
        // Extra space above each sidebar section, on top of the column spacing.
        const SECTION_GAP: f32 = 6.0;

        let tab = |label, tab| {
            button(label)
                .width(Fill)
                .style(if self.tab == tab && self.editor.is_none() {
                    button::primary
                } else {
                    ui::gray
                })
                .on_press(Message::Tab(tab))
        };

        let header = ui::heading("Social Activities!");

        let categories = column(CATEGORIES.iter().map(|c| {
            focusable::checkbox(
                self.filters.categories.contains(c.slug),
                c.name,
                move |on| Message::Category(c.slug, on),
            )
        }))
        .spacing(6);

        let tags: Element<'_, Message> = if self.tags.is_empty() {
            ui::muted("No tags yet").into()
        } else {
            row(self.tags.iter().map(|t| {
                ui::filter_tag(
                    t,
                    self.filters.tags.contains(t),
                    self.tag_presence(t),
                    Message::ToggleTag(t.clone()),
                )
            }))
            .spacing(6)
            .wrap()
            .vertical_spacing(6)
            .into()
        };

        let mut search_header = row![ui::bold("Search"), space::horizontal()].align_y(iced::Center);
        if self.filters.is_active() {
            search_header = search_header.push(
                button(iced::widget::text("Reset").size(14))
                    // No vertical padding, so it's shorter than the "Search" label
                    // and showing it doesn't change the header's height.
                    .padding([0, 6])
                    .style(button::text)
                    .on_press(Message::ClearFilters),
            );
        }

        let sidebar = column![
            header,
            tab("Events", Tab::Events),
            tab("Groups", Tab::Groups),
            rule::horizontal(1),
            search_header,
            text_input("Search…", &self.filters.search)
                .id(SEARCH_INPUT)
                .on_input(Message::Search),
            location::area_picker(
                self.filters.area,
                self.open_dropdown == Some(location::Dropdown::Area)
            ),
            space::vertical().height(SECTION_GAP),
            ui::muted("Categories"),
            categories,
            space::vertical().height(SECTION_GAP),
            ui::muted("Tags"),
            tags,
        ]
        .spacing(10)
        .padding(16);

        // Lists are managed rarely, so they sit at the bottom, outside the scroll.
        let footer = column![rule::horizontal(1), tab("Lists", Tab::Lists)]
            .spacing(10)
            .padding(iced::Padding::new(0.0).left(16).right(16).bottom(16));

        column![scrollable(sidebar).height(Fill), footer]
            .width(240)
            .height(Fill)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_know_when_they_narrow_results() {
        let mut filters = Filters::default();
        assert!(!filters.is_active());
        filters.search = "  ".into();
        assert!(!filters.is_active(), "blank search doesn't count");
        filters.search = "run".into();
        assert!(filters.is_active());
        filters = Filters::default();
        filters.tags.insert("running".into());
        assert!(filters.is_active());
        filters = Filters {
            area: Area::Region(sdsa_core::catalog::Region::NorthCounty),
            ..Filters::default()
        };
        assert!(filters.is_active());
    }

    #[test]
    fn lists_turned_off_hide_their_groups_events_and_tags() {
        let mut store = Store::in_memory().unwrap();
        let list = store.default_list().unwrap();
        let mut g = Group::new(&list.id);
        g.name = "Trivia".into();
        g.tags = vec!["trivia".into()];
        store.save_group(&g).unwrap();
        let mut e = Event::new(&list.id, recurrence::now_local() + TimeDelta::days(1));
        e.group_id = Some(g.id.clone());
        store.save_event(&e).unwrap();
        let mut app = App::new(store, None);
        assert_eq!(app.occurrences.len(), 1);
        assert_eq!(app.tags, ["trivia"]);
        assert!(!app.not_offered().contains(&g.id));

        let _ = app.update(Message::Lists(lists::Msg::SetOn(list.id.clone(), false)));
        assert!(!app.is_on(&list.id));
        assert!(app.occurrences.is_empty());
        assert!(app.tags.is_empty());
        assert!(app.not_offered().contains(&g.id));

        let _ = app.update(Message::Lists(lists::Msg::SetOn(list.id.clone(), true)));
        assert_eq!(app.occurrences.len(), 1);
    }

    #[test]
    fn catalog_lists_are_added_on_or_off() {
        let file = |id: &str, name: &str| {
            let mut source = Store::in_memory().unwrap();
            let list = source.create_list(name).unwrap();
            let mut file = source.export(&list.id).unwrap();
            file.id = id.into();
            file.to_json()
        };
        let entry = |id: &str, default_on: bool| IndexEntry {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            region: "SanDiegoCounty".into(),
            url: format!("https://x.org/{id}.json"),
            default_on,
        };
        let mut app = App::new(Store::in_memory().unwrap(), None);
        let _ = app.update(Message::IndexListsFetched {
            first: true,
            fetched: vec![
                (entry("clubs", true), Ok(file("clubs", "Clubs"))),
                (entry("meetup", false), Ok(file("meetup", "Meetup"))),
                (entry("broken", true), Err("offline".into())),
            ],
        });
        assert!(app.is_on("clubs"));
        assert!(!app.is_on("meetup"));
        assert!(app.list_infos.iter().all(|l| l.id != "broken"));
        let (status, is_error) = app.status.clone().unwrap();
        assert!(is_error);
        assert!(status.contains("Added “Clubs”"), "{status}");
        assert!(status.contains("“Meetup”, turned off"), "{status}");
        assert!(
            status.contains("Couldn't add “broken”: offline"),
            "{status}"
        );
    }

    #[test]
    fn lists_new_to_the_catalog_after_the_first_launch_start_off() {
        let mut app = App::new(Store::in_memory().unwrap(), None);
        let index = r#"{"format": 1, "lists": []}"#;
        let _ = app.update(Message::IndexFetched(Ok(index.into())));
        assert_eq!(app.store.setting(INDEX_READ).unwrap().as_deref(), Some("1"));

        let mut source = Store::in_memory().unwrap();
        let list = source.create_list("Pickleball").unwrap();
        let json = source.export(&list.id).unwrap().to_json();
        let entry = IndexEntry {
            id: list.id.clone(),
            name: "Pickleball".into(),
            description: String::new(),
            region: String::new(),
            url: "https://x.org/pickleball.json".into(),
            default_on: true,
        };
        let _ = app.update(Message::IndexListsFetched {
            first: false,
            fetched: vec![(entry, Ok(json))],
        });
        assert!(!app.is_on(&list.id));
        let (status, is_error) = app.status.clone().unwrap();
        assert!(!is_error);
        assert_eq!(
            status,
            "A new list was added to the app: “Pickleball”. It's off for now; turn it on in Lists."
        );
    }

    #[test]
    fn tags_in_results_follow_the_other_filters_and_panel() {
        let mut store = Store::in_memory().unwrap();
        let list = store.default_list().unwrap();
        let mut g = Group::new(&list.id);
        g.name = "Chess club".into();
        g.tags = vec!["chess".into()];
        store.save_group(&g).unwrap();
        let tomorrow = recurrence::now_local() + TimeDelta::days(1);
        let mut blitz = Event::new(&list.id, tomorrow);
        blitz.title = "Blitz".into();
        blitz.group_id = Some(g.id.clone());
        blitz.tags = vec!["outdoors".into()];
        store.save_event(&blitz).unwrap();
        let mut trivia = Event::new(&list.id, tomorrow);
        trivia.title = "Trivia".into();
        trivia.tags = vec!["trivia".into()];
        store.save_event(&trivia).unwrap();
        let mut app = App::new(store, None);
        let tags = |app: &App| {
            let mut tags: Vec<String> = app
                .tags_in_results()
                .into_iter()
                .map(String::from)
                .collect();
            tags.sort();
            tags
        };

        assert_eq!(tags(&app), ["chess", "outdoors", "trivia"]);
        let _ = app.update(Message::Search("blitz".into()));
        assert_eq!(
            tags(&app),
            ["chess", "outdoors"],
            "includes the group's tags"
        );
        let _ = app.update(Message::ToggleTag("trivia".into()));
        assert!(
            tags(&app).is_empty(),
            "nothing matches, so every tag is grayed"
        );

        let _ = app.update(Message::ClearFilters);
        let _ = app.update(Message::Tab(Tab::Groups));
        assert_eq!(tags(&app), ["chess"]);
    }

    #[test]
    fn tags_fade_when_they_gain_or_lose_results() {
        let mut store = Store::in_memory().unwrap();
        let list = store.default_list().unwrap();
        let mut e = Event::new(&list.id, recurrence::now_local() + TimeDelta::days(1));
        e.title = "Trivia".into();
        e.tags = vec!["trivia".into()];
        store.save_event(&e).unwrap();
        let mut app = App::new(store, None);
        assert_eq!(app.tag_presence("trivia"), 1.0, "starts where it belongs");
        assert!(!app.tag_fades["trivia"].is_animating(app.now));

        let _ = app.update(Message::Search("bingo".into()));
        assert!(!app.tag_fades["trivia"].value());
        assert!(app.tag_fades["trivia"].is_animating(app.now));
        let later = app.now + TAG_FADE;
        assert_eq!(app.tag_fades["trivia"].interpolate(0.0, 1.0, later), 0.0);
    }

    #[test]
    fn filters_are_there_after_a_restart() {
        let mut app = App::new(Store::in_memory().unwrap(), None);
        let _ = app.update(Message::Category("chill", true));
        let _ = app.update(Message::Horizon(Horizon::TenDays));
        let _ = app.update(Message::Search("trivia".into()));

        let app = App::new(app.store, None);
        assert!(app.filters.categories.contains("chill"));
        assert_eq!(app.horizon, Horizon::TenDays);
        assert_eq!(app.filters.search, "");
    }

    #[test]
    fn hidden_and_visible_views_split_groups() {
        let shown = Group::new("list");
        let hidden_group = Group::new("list");
        let hidden = HashSet::from([hidden_group.id.clone()]);
        let filters = Filters::default();
        assert!(filters.group(&shown, &hidden, false));
        assert!(!filters.group(&shown, &hidden, true));
        assert!(filters.group(&hidden_group, &hidden, true));
        assert!(!filters.group(&hidden_group, &hidden, false));
    }

    #[test]
    fn tags_must_all_match_and_include_group_tags() {
        let mut group = Group::new("list");
        group.tags = vec!["beginner friendly".into()];
        let mut event = Event::new("list", Default::default());
        event.tags = vec!["night".into()];
        let hidden = HashSet::new();

        let mut filters = Filters::default();
        filters.tags.insert("beginner friendly".into());
        assert!(filters.event(&event, Some(&group), &hidden));
        filters.tags.insert("night".into());
        assert!(filters.event(&event, Some(&group), &hidden));
        assert!(!filters.group(&group, &hidden, false));
        filters.tags.insert("dogs".into());
        assert!(!filters.event(&event, Some(&group), &hidden));
    }
}
