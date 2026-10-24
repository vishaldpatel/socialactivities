//! A month calendar (weeks start on Monday) for picking a date, usable
//! entirely from the keyboard:
//! arrows move by day or week, Page Up/Down by month, Enter picks, Escape closes.

use chrono::{Datelike, Months, NaiveDate, TimeDelta};
use iced::keyboard::{Key, key::Named};
use iced::widget::{Column, button, column, container, row, text};
use iced::{Center, Element, Fill, Theme};

use crate::ui;

/// Focusing this id (which no widget has) takes focus away from text fields,
/// so the arrow keys reach the calendar instead of moving a text cursor.
pub const FOCUS_NOTHING: &str = "calendar";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Move the highlighted day by this many days.
    Move(i64),
    /// Move the highlighted day by this many months.
    Month(i32),
    Pick(NaiveDate),
    Close,
}

/// What a key does while the calendar is open, if anything.
pub fn key_action(key: &Key, cursor: NaiveDate) -> Option<Action> {
    match key.as_ref() {
        Key::Named(Named::ArrowLeft) => Some(Action::Move(-1)),
        Key::Named(Named::ArrowRight) => Some(Action::Move(1)),
        Key::Named(Named::ArrowUp) => Some(Action::Move(-7)),
        Key::Named(Named::ArrowDown) => Some(Action::Move(7)),
        Key::Named(Named::PageUp) => Some(Action::Month(-1)),
        Key::Named(Named::PageDown) => Some(Action::Month(1)),
        Key::Named(Named::Enter | Named::Space) => Some(Action::Pick(cursor)),
        Key::Named(Named::Escape) => Some(Action::Close),
        _ => None,
    }
}

/// `cursor` after a move. Moving by months keeps the day where it can,
/// otherwise the month's last day.
pub fn moved(cursor: NaiveDate, action: Action) -> NaiveDate {
    match action {
        Action::Move(days) => cursor.checked_add_signed(TimeDelta::days(days)),
        Action::Month(m) if m >= 0 => cursor.checked_add_months(Months::new(m as u32)),
        Action::Month(m) => cursor.checked_sub_months(Months::new(m.unsigned_abs())),
        Action::Pick(_) | Action::Close => Some(cursor),
    }
    .unwrap_or(cursor)
}

/// The 6 weeks (Monday first) shown for the month containing `date`.
fn grid(date: NaiveDate) -> Vec<NaiveDate> {
    let first = date.with_day(1).unwrap_or(date);
    let start = first - TimeDelta::days(i64::from(first.weekday().num_days_from_monday()));
    (0..42).map(|i| start + TimeDelta::days(i)).collect()
}

pub fn view<'a, M: Clone + 'a>(
    cursor: NaiveDate,
    selected: Option<NaiveDate>,
    on: impl Fn(Action) -> M + 'a,
) -> Element<'a, M> {
    let nav = |label: &'static str, months: i32| {
        button(text(label).size(14))
            .style(ui::gray)
            .on_press(on(Action::Month(months)))
    };
    let header = row![
        nav("‹", -1),
        text(cursor.format("%B %Y").to_string())
            .width(Fill)
            .align_x(Center),
        nav("›", 1),
    ]
    .align_y(Center);

    let cell = 36;
    let weekdays = row(["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
        .map(|d| ui::muted(d).width(cell).align_x(Center).into()));

    let days = grid(cursor);
    let weeks = Column::with_children(days.chunks(7).map(|week| {
        row(week.iter().map(|&day| {
            let in_month = day.month() == cursor.month();
            let style = if day == cursor {
                button::primary
            } else if Some(day) == selected {
                ui::gray
            } else {
                button::text
            };
            let label = text(day.day().to_string())
                .size(14)
                .width(Fill)
                .align_x(Center);
            let label = if in_month { label } else { label.style(dim) };
            button(label)
                .width(cell)
                .padding([4, 0])
                .style(style)
                .on_press(on(Action::Pick(day)))
                .into()
        }))
        .into()
    }));

    container(
        column![
            header,
            weekdays,
            weeks,
            ui::muted("Arrows move · Page Up/Down change month · Enter picks · Esc closes"),
        ]
        .spacing(6)
        .width(cell as f32 * 7.0),
    )
    .padding(10)
    .style(container::bordered_box)
    .into()
}

fn dim(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.extended_palette().background.strong.color),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn grid_starts_on_the_monday_before_the_first() {
        let days = grid(d(2026, 10, 24));
        assert_eq!(days[0], d(2026, 9, 28));
        assert_eq!(days.len(), 42);
        assert!(days.contains(&d(2026, 10, 31)));
    }

    #[test]
    fn keys_move_the_cursor() {
        let c = d(2026, 1, 31);
        let step = |k: Key| moved(c, key_action(&k, c).unwrap());
        assert_eq!(step(Key::Named(Named::ArrowRight)), d(2026, 2, 1));
        assert_eq!(step(Key::Named(Named::ArrowUp)), d(2026, 1, 24));
        assert_eq!(
            step(Key::Named(Named::PageDown)),
            d(2026, 2, 28),
            "clamps to month end"
        );
        assert_eq!(step(Key::Named(Named::PageUp)), d(2025, 12, 31));
        assert_eq!(
            key_action(&Key::Named(Named::Enter), c),
            Some(Action::Pick(c))
        );
        assert_eq!(
            key_action(&Key::Named(Named::Escape), c),
            Some(Action::Close)
        );
        assert_eq!(key_action(&Key::Character("x".into()), c), None);
    }
}
