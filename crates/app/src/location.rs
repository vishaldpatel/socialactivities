//! Location pickers: places listed under region headings. The search panel can
//! pick a heading to cover a whole region; the editors can only pick places.

use iced::widget::pick_list;
use iced::{Element, Fill};
use sdsa_core::catalog::{COUNTYWIDE, Neighborhood, Region, find_neighborhood};

use crate::app::Message;

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
}

impl std::fmt::Display for Area {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Area::All => f.write_str("ALL OF SD COUNTY"),
            Area::Region(r) => f.write_str(&r.name().to_uppercase()),
            Area::Place(n) => write!(f, "    {}", n.name),
        }
    }
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
pub fn area_picker<'a>(selected: Area) -> Element<'a, Message> {
    pick_list(entries(), Some(selected), Message::Area)
        .width(Fill)
        .into()
}

/// The editors' picker, where only places can be chosen; headings do nothing.
pub fn place_picker<'a>(
    selected: Option<Neighborhood>,
    on_select: impl Fn(Neighborhood) -> Message + 'a,
) -> Element<'a, Message> {
    pick_list(
        entries(),
        selected.map(Area::Place),
        move |entry| match entry {
            Area::Place(n) => on_select(n),
            Area::All | Area::Region(_) => Message::Noop,
        },
    )
    .placeholder("Choose a neighborhood")
    .width(Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdsa_core::catalog::NEIGHBORHOODS;

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
