//! The Lists panel: create, edit, export, add, remove and update lists.

use std::collections::HashMap;
use std::path::PathBuf;

use iced::futures::channel::oneshot;
use iced::widget::{Column, button, column, row, space, text, text_input};
use iced::{Center, Element, Fill};
use sdsa_core::{ListFile, ListInfo};

use crate::app::{App, Message};
use crate::{focusable, ui};

/// The largest list file accepted from a URL.
const MAX_DOWNLOAD: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone)]
pub enum Msg {
    NewList,
    Edit(String),
    EditName(String),
    EditDescription(String),
    EditUpdateUrl(String),
    SaveEdit,
    CancelEdit,
    Export(String),
    Exported(String, Option<PathBuf>),
    Location(String),
    Maintain(bool),
    Browse,
    Browsed(Option<PathBuf>),
    Add,
    Fetched {
        location: String,
        result: Result<String, String>,
    },
    Remove(String),
    CancelRemove,
    Check(String),
    CheckAll,
    Checked {
        id: String,
        result: Result<String, String>,
    },
    Update(String),
}

/// A list's details as typed in its edit form.
pub struct ListEdit {
    pub id: String,
    pub name: String,
    pub description: String,
    pub update_url: String,
}

impl ListEdit {
    pub fn of(list: &ListInfo) -> Self {
        Self {
            id: list.id.clone(),
            name: list.name.clone(),
            description: list.description.clone(),
            update_url: list.update_url.clone().unwrap_or_default(),
        }
    }

    /// Checks the form, returning a message for the first problem.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.name.trim().is_empty() {
            return Err("A list needs a name.");
        }
        let url = self.update_url.trim();
        if !url.is_empty() && !is_url(url) {
            return Err("The update URL must start with https:// or http://.");
        }
        Ok(())
    }
}

pub enum UpdateState {
    Checking,
    UpToDate,
    Available(Box<ListFile>),
    Failed(String),
}

/// The Lists panel's own state.
#[derive(Default)]
pub struct Lists {
    /// URL or file path to add a list from.
    pub location: String,
    /// The list being added is one you maintain, so it should be editable.
    pub maintain: bool,
    pub adding: bool,
    /// The list whose details are being edited.
    pub editing: Option<ListEdit>,
    /// The list whose Remove button was pressed once.
    pub confirm_remove: Option<String>,
    pub updates: HashMap<String, UpdateState>,
}

pub fn is_url(location: &str) -> bool {
    location.starts_with("https://") || location.starts_with("http://")
}

/// Runs a blocking job (a download, file read or dialog) on its own thread.
pub async fn in_background<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(job());
    });
    rx.await.expect("background job panicked")
}

/// Reads a list file from a URL or a local path.
pub fn read_location(location: &str) -> Result<String, String> {
    if is_url(location) {
        ureq::get(location)
            .call()
            .map_err(|e| format!("Couldn't download {location}: {e}"))?
            .body_mut()
            .with_config()
            .limit(MAX_DOWNLOAD)
            .read_to_string()
            .map_err(|e| format!("Couldn't read {location}: {e}"))
    } else {
        std::fs::read_to_string(location).map_err(|e| format!("Couldn't open {location}: {e}"))
    }
}

/// A file-name-friendly version of a list's name.
pub fn file_name(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("{}.json", if slug.is_empty() { "list" } else { &slug })
}

pub fn pick_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Add a list")
        .add_filter("Lists", &["json"])
        .pick_file()
}

pub fn save_file(name: String) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export list")
        .add_filter("Lists", &["json"])
        .set_file_name(name)
        .save_file()
}

fn gray<'a>(label: &'a str, msg: Msg) -> Element<'a, Message> {
    focusable::button(
        button(text(label).size(13)).style(ui::gray),
        Message::Lists(msg),
    )
}

pub fn view(app: &App) -> Element<'_, Message> {
    let state = &app.lists;
    let checkable = app
        .list_infos
        .iter()
        .any(|l| !l.owned && l.update_location().is_some());

    let mut header = row![
        focusable::button_with_id(
            button("New List"),
            Message::Lists(Msg::NewList),
            crate::app::RESULTS_START,
        ),
        space::horizontal(),
    ]
    .spacing(8)
    .align_y(Center);
    if checkable {
        header = header.push(focusable::button(
            button("Check All for Updates").style(ui::gray),
            Message::Lists(Msg::CheckAll),
        ));
    }

    let add_label = button(if state.adding {
        "Adding…"
    } else {
        "Add List"
    });
    // Only reachable with Tab while it can be pressed.
    let add_button: Element<'_, Message> = if !state.adding && !state.location.trim().is_empty() {
        focusable::button(add_label, Message::Lists(Msg::Add))
    } else {
        add_label.into()
    };
    let add = ui::card(
        column![
            ui::bold("Add a list"),
            row![
                text_input("https://… or a file path", &state.location)
                    .on_input(|s| Message::Lists(Msg::Location(s)))
                    .on_submit(Message::Lists(Msg::Add)),
                focusable::button(
                    button("Browse…").style(ui::gray),
                    Message::Lists(Msg::Browse)
                ),
                add_button,
            ]
            .spacing(8)
            .align_y(Center),
            focusable::checkbox(
                state.maintain,
                "I maintain this list (make it editable here)",
                |on| Message::Lists(Msg::Maintain(on)),
            ),
        ]
        .spacing(10),
    );

    let cards = Column::with_children(app.list_infos.iter().map(|l| card(app, l))).spacing(10);

    column![
        header,
        iced::widget::scrollable(column![add, cards].spacing(14).padding([0, 12]))
            .id(crate::app::MAIN_SCROLL)
            .height(Fill)
    ]
    .spacing(12)
    .into()
}

fn card<'a>(app: &'a App, list: &'a ListInfo) -> Element<'a, Message> {
    let state = &app.lists;
    let groups = app.groups.iter().filter(|g| g.list_id == list.id).count();
    let events = app.events.iter().filter(|e| e.list_id == list.id).count();

    if let Some(edit) = state.editing.as_ref().filter(|e| e.id == list.id) {
        return edit_form(edit);
    }

    let title = row![
        ui::bold(list.name.as_str()).size(17),
        ui::chip(if list.owned { "Yours" } else { "Added" }),
    ]
    .spacing(8)
    .align_y(Center);

    let updated = list
        .updated_at
        .with_timezone(&sdsa_core::recurrence::TZ)
        .format("%b %-d, %Y");
    let mut body = column![
        title,
        ui::muted(format!(
            "{groups} group{} · {events} event{} · Updated {updated}",
            if groups == 1 { "" } else { "s" },
            if events == 1 { "" } else { "s" },
        )),
    ]
    .spacing(6);
    if !list.description.is_empty() {
        body = body.push(text(list.description.as_str()).size(14));
    }
    if list.owned {
        if let Some(url) = &list.update_url {
            body = body.push(ui::muted(format!("Published at {url}")));
        }
    } else {
        body = body.push(ui::muted(match list.update_location() {
            Some(location) => format!("Updates from {location}"),
            None => "Built in; updated with the app".into(),
        }));
    }

    match state.updates.get(&list.id) {
        Some(UpdateState::Checking) => body = body.push(ui::muted("Checking for updates…")),
        Some(UpdateState::UpToDate) => body = body.push(ui::muted("Up to date")),
        Some(UpdateState::Failed(e)) => body = body.push(ui::banner(e, true)),
        Some(UpdateState::Available(file)) => {
            let when = file
                .updated_at
                .with_timezone(&sdsa_core::recurrence::TZ)
                .format("%b %-d, %Y");
            body = body.push(
                row![
                    ui::banner(format!("A newer version from {when} is available"), false),
                    focusable::button(
                        button(text("Update").size(13)),
                        Message::Lists(Msg::Update(list.id.clone()))
                    ),
                ]
                .spacing(8)
                .align_y(Center),
            );
        }
        None => {}
    }

    let mut actions = row![].spacing(6).align_y(Center);
    if state.confirm_remove.as_deref() == Some(&list.id) {
        actions = actions
            .push(text(format!("Remove “{}” and everything in it?", list.name)).size(14))
            .push(focusable::button(
                button(text("Yes, Remove").size(13)).style(button::danger),
                Message::Lists(Msg::Remove(list.id.clone())),
            ))
            .push(gray("Cancel", Msg::CancelRemove));
    } else {
        if list.owned {
            actions = actions.push(gray("Edit", Msg::Edit(list.id.clone())));
        } else if list.update_location().is_some() {
            actions = actions.push(gray("Check for Updates", Msg::Check(list.id.clone())));
        }
        actions = actions
            .push(gray("Export…", Msg::Export(list.id.clone())))
            .push(space::horizontal())
            .push(focusable::button(
                button(text("Remove").size(13)).style(button::text),
                Message::Lists(Msg::Remove(list.id.clone())),
            ));
    }
    body = body.push(actions);

    ui::card(body)
}

fn edit_form(edit: &ListEdit) -> Element<'_, Message> {
    let field = |label, input: iced::widget::TextInput<'static, Message>| {
        column![ui::muted(label), input].spacing(4)
    };
    ui::card(
        column![
            field(
                "Name",
                text_input("List name", &edit.name)
                    .on_input(|s| Message::Lists(Msg::EditName(s)))
                    .on_submit(Message::Lists(Msg::SaveEdit)),
            ),
            field(
                "Description",
                text_input("What's in this list", &edit.description)
                    .on_input(|s| Message::Lists(Msg::EditDescription(s)))
                    .on_submit(Message::Lists(Msg::SaveEdit)),
            ),
            field(
                "Update URL (optional): where you'll publish this list, so people can check for newer versions",
                text_input("https://…", &edit.update_url)
                    .on_input(|s| Message::Lists(Msg::EditUpdateUrl(s)))
                    .on_submit(Message::Lists(Msg::SaveEdit)),
            ),
            row![
                focusable::button(button(text("Save").size(13)), Message::Lists(Msg::SaveEdit)),
                gray("Cancel", Msg::CancelEdit),
            ]
            .spacing(8),
        ]
        .spacing(10),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_from_list_names() {
        assert_eq!(
            file_name("San Diego Social Activities"),
            "san-diego-social-activities.json"
        );
        assert_eq!(file_name("  Surf & Sand!! "), "surf-sand.json");
        assert_eq!(file_name("???"), "list.json");
    }

    #[test]
    fn validates_list_edits() {
        let mut edit = ListEdit {
            id: "x".into(),
            name: "Beach".into(),
            description: String::new(),
            update_url: String::new(),
        };
        assert!(edit.validate().is_ok());
        edit.update_url = "example.com/list.json".into();
        assert!(edit.validate().is_err());
        edit.update_url = "https://example.com/list.json".into();
        assert!(edit.validate().is_ok());
        edit.name = "  ".into();
        assert!(edit.validate().is_err());
    }

    #[test]
    fn recognizes_urls() {
        assert!(is_url("https://example.com/list.json"));
        assert!(!is_url("/home/me/list.json"));
    }

    #[test]
    fn reads_local_files() {
        let dir = std::env::temp_dir().join(format!("sdsa-lists-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("list.json");
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(read_location(path.to_str().unwrap()).unwrap(), "{}");
        assert!(read_location(dir.join("missing.json").to_str().unwrap()).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
