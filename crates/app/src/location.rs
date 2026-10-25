//! Location pickers: places listed under region headings, each region in its
//! own color. The search panel can pick a heading to cover a whole region; the
//! editors can only pick places.
//!
//! Iced's dropdown draws every entry in the same color, so this is a button
//! that opens a pop-up list (iced_aw's `DropDown`). Like the other dropdowns it
//! can be reached with Tab: Up/Down step through the entries, Enter or Space
//! open and close it, Esc closes it.

use iced::keyboard::{self, key::Named};
use iced::widget::{Column, button, container, row, scrollable, text};
use iced::{Center, Color, Element, Fill, Theme};
use iced_aw::DropDown;
use sdsa_core::catalog::{COUNTYWIDE, Neighborhood, Region, find_neighborhood};

use crate::app::Message;
use crate::{focusable, ui};

/// An entry in the location list, and the search panel's area filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Area {
    #[default]
    All,
    Region(Region),
    Place(Neighborhood),
}

impl Area {
    /// Whether a record in neighborhood `slug` is in this area. Countywide
    /// records count as being everywhere.
    pub fn contains(self, slug: &str) -> bool {
        match self {
            Area::All => true,
            _ if slug == COUNTYWIDE => true,
            Area::Region(r) => find_neighborhood(slug).and_then(|n| n.region) == Some(r),
            Area::Place(n) => n.slug == slug,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Area::All => "All of SD County",
            Area::Region(r) => r.name(),
            Area::Place(n) => n.name,
        }
    }

    fn color(self) -> Option<Color> {
        match self {
            Area::All => None,
            Area::Region(r) => Some(region_color(r)),
            Area::Place(n) => n.region.map(region_color),
        }
    }
}

/// Each region's color, chosen to read well on the app's dark backgrounds.
pub fn region_color(region: Region) -> Color {
    match region {
        Region::NorthCounty => Color::from_rgb8(0x7a, 0xe6, 0xc0), // seafoam
        Region::SanDiegoCity => Color::from_rgb8(0xb0, 0xd2, 0xff), // sky blue
        Region::EastCounty => Color::from_rgb8(0xf2, 0xc9, 0x6b),  // amber
        Region::SouthCounty => Color::from_rgb8(0xfb, 0xbd, 0xce), // pink
    }
}

/// The color for a record's neighborhood slug; none for countywide or unknown.
pub fn color_of(slug: &str) -> Option<Color> {
    find_neighborhood(slug)
        .and_then(|n| n.region)
        .map(region_color)
}

/// Height of the open list, which scrolls.
const LIST_HEIGHT: f32 = 260.0;

/// Which location dropdown is open; at most one is shown at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dropdown {
    /// The search panel's area filter.
    Area,
    /// The neighborhood field of the form being edited.
    Place,
}

fn entries() -> Vec<Area> {
    // "Countywide / Various" isn't offered; records that have it match every area.
    let mut entries = vec![Area::All];
    for region in Region::ALL {
        entries.push(Area::Region(region));
        entries.extend(region.neighborhoods().map(Area::Place));
    }
    entries
}

/// The search panel's picker, where headings select a whole region or the county.
pub fn area_picker<'a>(selected: Area, open: bool) -> Element<'a, Message> {
    picker(
        Dropdown::Area,
        Some(selected),
        "",
        open,
        |_| true,
        Message::Area,
    )
}

/// The editors' picker, where only places can be chosen.
pub fn place_picker<'a>(
    selected: Option<Neighborhood>,
    open: bool,
    on_select: impl Fn(Neighborhood) -> Message + Clone + 'a,
) -> Element<'a, Message> {
    picker(
        Dropdown::Place,
        selected.map(Area::Place),
        "Choose a neighborhood",
        open,
        |a| matches!(a, Area::Place(_)),
        move |a| match a {
            Area::Place(n) => on_select(n),
            Area::All | Area::Region(_) => Message::Noop,
        },
    )
}

fn picker<'a>(
    which: Dropdown,
    selected: Option<Area>,
    placeholder: &'static str,
    open: bool,
    selectable: fn(Area) -> bool,
    on_pick: impl Fn(Area) -> Message + Clone + 'a,
) -> Element<'a, Message> {
    let entries = entries();

    let label = match selected {
        Some(area) => text(area.name()).color_maybe(area.color()),
        None => text(placeholder).style(|theme: &Theme| text::Style {
            color: Some(theme.extended_palette().background.strongest.color),
        }),
    };
    let toggle = Message::Dropdown((!open).then_some(which));
    let underlay = button(row![label.width(Fill), text("▾")].align_y(Center))
        .width(Fill)
        .padding([5, 10])
        .style(box_style)
        .on_press(toggle.clone());

    // Keys while the box is highlighted.
    let current = selected.and_then(|s| entries.iter().position(|e| *e == s));
    let step_entries = entries.clone();
    let step_pick = on_pick.clone();
    let underlay = focusable::focusable(underlay, move |key| match key.as_ref() {
        keyboard::Key::Named(Named::Enter | Named::Space) => Some(toggle.clone()),
        keyboard::Key::Named(Named::Escape) if open => Some(Message::Dropdown(None)),
        keyboard::Key::Named(Named::ArrowUp) => {
            step(&step_entries, current, -1, selectable).map(&step_pick)
        }
        keyboard::Key::Named(Named::ArrowDown) => {
            step(&step_entries, current, 1, selectable).map(&step_pick)
        }
        _ => None,
    });

    let list = Column::with_children(entries.into_iter().map(|entry| {
        entry_view(
            entry,
            Some(entry) == selected,
            selectable(entry),
            on_pick.clone(),
        )
    }));
    // Short enough to usually fit below the box; it opens upward when it can't.
    let overlay = container(scrollable(list).height(LIST_HEIGHT))
        .padding(4)
        .width(Fill)
        .style(container::bordered_box);

    DropDown::new(underlay, overlay, open)
        .on_dismiss(Message::Dropdown(None))
        .into()
}

/// The next selectable entry from `current` in direction `dir`, if any.
fn step(
    entries: &[Area],
    current: Option<usize>,
    dir: isize,
    selectable: fn(Area) -> bool,
) -> Option<Area> {
    let mut i = match current {
        Some(i) => i as isize,
        None if dir > 0 => -1,
        None => entries.len() as isize,
    };
    loop {
        i += dir;
        let entry = *entries.get(usize::try_from(i).ok()?)?;
        if selectable(entry) {
            return Some(entry);
        }
    }
}

fn entry_view<'a>(
    entry: Area,
    selected: bool,
    selectable: bool,
    on_pick: impl Fn(Area) -> Message + 'a,
) -> Element<'a, Message> {
    let heading = !matches!(entry, Area::Place(_));
    let label = if heading {
        ui::bold(entry.name().to_uppercase()).size(13)
    } else {
        text(entry.name()).size(15)
    }
    .color_maybe(entry.color());
    // Places are indented under their region's heading.
    let padding = if heading { [4, 8] } else { [3, 24] };

    if !selectable {
        return container(label).padding(padding).width(Fill).into();
    }
    button(label)
        .padding(padding)
        .width(Fill)
        .style(move |theme: &Theme, status| {
            let palette = theme.extended_palette();
            let background = match status {
                _ if selected => Some(palette.primary.weak.color.into()),
                button::Status::Hovered | button::Status::Pressed => {
                    Some(palette.background.strong.color.into())
                }
                _ => None,
            };
            button::Style {
                background,
                text_color: palette.background.base.text,
                border: iced::border::rounded(2),
                ..button::Style::default()
            }
        })
        .on_press(Message::Picked(Box::new(on_pick(entry))))
        .into()
}

/// The closed box, looking like Iced's dropdown.
fn box_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let border_color = match status {
        button::Status::Hovered | button::Status::Pressed => palette.primary.strong.color,
        _ => palette.background.strong.color,
    };
    button::Style {
        background: Some(palette.background.weak.color.into()),
        text_color: palette.background.base.text,
        border: iced::Border {
            color: border_color,
            width: 1.0,
            radius: 2.0.into(),
        },
        ..button::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdsa_core::catalog::NEIGHBORHOODS;

    /// WCAG contrast ratio between two colors.
    fn contrast(a: Color, b: Color) -> f32 {
        let lum = |c: Color| {
            let ch = |v: f32| {
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * ch(c.r) + 0.7152 * ch(c.g) + 0.0722 * ch(c.b)
        };
        let (hi, lo) = {
            let (x, y) = (lum(a), lum(b));
            if x > y { (x, y) } else { (y, x) }
        };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn region_colors_read_on_dark_backgrounds() {
        let theme = Theme::Dark;
        let palette = theme.extended_palette();
        // The page, cards, the dropdown box and a hovered entry.
        let backgrounds = [
            palette.background.base.color,
            palette.background.weakest.color,
            palette.background.weak.color,
            palette.background.strong.color,
        ];
        for region in Region::ALL {
            for bg in backgrounds {
                let c = contrast(region_color(region), bg);
                assert!(c >= 4.5, "{region:?} on {bg:?}: {c:.2}");
            }
        }
    }

    #[test]
    fn lists_every_regional_place_once_under_a_heading() {
        let entries = entries();
        assert_eq!(entries[0], Area::All);
        let places = entries
            .iter()
            .filter(|e| matches!(e, Area::Place(_)))
            .count();
        let regional = NEIGHBORHOODS.iter().filter(|n| n.region.is_some()).count();
        assert_eq!(places, regional);
        assert_eq!(entries.len(), regional + 1 + Region::ALL.len());
        assert!(
            !entries
                .iter()
                .any(|e| matches!(e, Area::Place(n) if n.slug == COUNTYWIDE))
        );
    }

    #[test]
    fn stepping_skips_headings_in_place_pickers() {
        let entries = entries();
        let places = |a: Area| matches!(a, Area::Place(_));
        let first_place = step(&entries, None, 1, places).unwrap();
        assert!(matches!(first_place, Area::Place(n) if n.region == Some(Region::NorthCounty)));
        // From the last North County place, Down skips the San Diego City heading.
        let last_north = entries
            .iter()
            .rposition(|e| matches!(e, Area::Place(n) if n.region == Some(Region::NorthCounty)))
            .unwrap();
        let next = step(&entries, Some(last_north), 1, places).unwrap();
        assert!(matches!(next, Area::Place(n) if n.region == Some(Region::SanDiegoCity)));
        // The area picker can land on headings.
        assert_eq!(
            step(&entries, Some(0), 1, |_| true),
            Some(Area::Region(Region::NorthCounty))
        );
        assert_eq!(step(&entries, Some(0), -1, |_| true), None);
    }

    #[test]
    fn areas_contain_their_places_and_countywide() {
        let encinitas = Area::Place(find_neighborhood("encinitas").unwrap());
        assert!(Area::All.contains("hillcrest"));
        assert!(Area::Region(Region::NorthCounty).contains("encinitas"));
        assert!(!Area::Region(Region::NorthCounty).contains("hillcrest"));
        assert!(Area::Region(Region::NorthCounty).contains(COUNTYWIDE));
        assert!(encinitas.contains("encinitas"));
        assert!(!encinitas.contains("carlsbad"));
        assert!(encinitas.contains(COUNTYWIDE));
    }
}
