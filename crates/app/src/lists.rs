//! The Lists panel: create, rename, export, add, remove and update lists.

use std::collections::HashMap;
use std::path::PathBuf;

use iced::futures::channel::oneshot;
use iced::widget::{Column, button, checkbox, column, row, space, text, text_input};
use iced::{Center, Element, Fill};
use sdsa_core::{ListFile, ListInfo};

use crate::app::{App, Message};
use crate::ui;

/// The largest list file accepted from a URL.
const MAX_DOWNLOAD: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone)]
pub enum Msg {
    NewList,
    Rename(String),
    RenameText(String),
    SaveRename,
    CancelRename,
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
    /// The list being renamed, and the name typed so far.
    pub renaming: Option<(String, String)>,
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
    button(text(label).size(13))
        .style(ui::gray)
        .on_press(Message::Lists(msg))
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let state = &app.lists;
    let checkable = app
        .list_infos
        .iter()
        .any(|l| !l.owned && l.update_location().is_some());

    let mut header = row![
        button("New List").on_press(Message::Lists(Msg::NewList)),
        space::horizontal(),
    ]
    .spacing(8)
    .align_y(Center);
    if checkable {
        header = header.push(
            button("Check All for Updates")
                .style(ui::gray)
                .on_press(Message::Lists(Msg::CheckAll)),
        );
    }

    let add_button = button(if state.adding {
        "Adding…"
    } else {
        "Add List"
    })
    .on_press_maybe(
        (!state.adding && !state.location.trim().is_empty()).then_some(Message::Lists(Msg::Add)),
    );
    let add = ui::card(
        column![
            ui::bold("Add a list"),
            row![
                text_input("https://… or a file path", &state.location)
                    .on_input(|s| Message::Lists(Msg::Location(s)))
                    .on_submit(Message::Lists(Msg::Add)),
                button("Browse…")
                    .style(ui::gray)
                    .on_press(Message::Lists(Msg::Browse)),
                add_button,
            ]
            .spacing(8)
            .align_y(Center),
            checkbox(state.maintain)
                .label("I maintain this list (make it editable here)")
                .on_toggle(|on| Message::Lists(Msg::Maintain(on)))
                .size(16)
                .text_size(14),
        ]
        .spacing(10),
    );

    let cards = Column::with_children(app.list_infos.iter().map(|l| card(app, l))).spacing(10);

    column![
        header,
        iced::widget::scrollable(column![add, cards].spacing(14).padding([0, 12])).height(Fill)
    ]
    .spacing(12)
    .into()
}

fn card<'a>(app: &'a App, list: &'a ListInfo) -> Element<'a, Message> {
    let state = &app.lists;
    let groups = app.groups.iter().filter(|g| g.list_id == list.id).count();
    let events = app.events.iter().filter(|e| e.list_id == list.id).count();

    let title: Element<'a, Message> = match &state.renaming {
        Some((id, name)) if *id == list.id => row![
            text_input("List name", name)
                .on_input(|s| Message::Lists(Msg::RenameText(s)))
                .on_submit(Message::Lists(Msg::SaveRename))
                .width(320),
            button(text("Save").size(13)).on_press(Message::Lists(Msg::SaveRename)),
            gray("Cancel", Msg::CancelRename),
        ]
        .spacing(8)
        .align_y(Center)
        .into(),
        _ => row![
            ui::bold(list.name.as_str()).size(17),
            ui::chip(if list.owned { "Yours" } else { "Added" }),
        ]
        .spacing(8)
        .align_y(Center)
        .into(),
    };

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
    if !list.owned {
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
                    button(text("Update").size(13))
                        .on_press(Message::Lists(Msg::Update(list.id.clone()))),
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
            .push(
                button(text("Yes, Remove").size(13))
                    .style(button::danger)
                    .on_press(Message::Lists(Msg::Remove(list.id.clone()))),
            )
            .push(gray("Cancel", Msg::CancelRemove));
    } else {
        if list.owned {
            actions = actions.push(gray("Rename", Msg::Rename(list.id.clone())));
        } else if list.update_location().is_some() {
            actions = actions.push(gray("Check for Updates", Msg::Check(list.id.clone())));
        }
        actions = actions
            .push(gray("Export…", Msg::Export(list.id.clone())))
            .push(space::horizontal())
            .push(
                button(text("Remove").size(13))
                    .style(button::text)
                    .on_press(Message::Lists(Msg::Remove(list.id.clone()))),
            );
    }
    body = body.push(actions);

    ui::card(body)
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
