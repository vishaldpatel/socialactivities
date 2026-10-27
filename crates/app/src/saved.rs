//! The sidebar's filters and the Events panel's grouping and range, saved
//! between runs. The search text isn't saved: it's typed, not chosen, and a
//! leftover search would hide things on the next start.
//!
//! Stored as `name=value` lines, so a value the app no longer knows (a
//! renamed neighborhood, say) is dropped without losing the rest.

use sdsa_core::catalog::{CATEGORIES, Region, find_neighborhood};

use crate::app::{Bucket, Filters, Horizon};
use crate::location::Area;

/// The settings key it's saved under.
pub const KEY: &str = "filters";

/// The view as a string to save.
pub fn encode(filters: &Filters, bucket: Bucket, horizon: Horizon) -> String {
    let mut lines = vec![format!("bucket={bucket:?}"), format!("horizon={horizon:?}")];
    match filters.area {
        Area::All => {}
        Area::Region(r) => lines.push(format!("region={r:?}")),
        Area::Place(n) => lines.push(format!("place={}", n.slug)),
    }
    // In catalog order, so the same choices always save the same way.
    for c in CATEGORIES
        .iter()
        .filter(|c| filters.categories.contains(c.slug))
    {
        lines.push(format!("category={}", c.slug));
    }
    for tag in &filters.tags {
        lines.push(format!("tag={tag}"));
    }
    lines.join("\n")
}

/// The option in `all` saved as `value`, by its Rust name.
fn by_name<T: std::fmt::Debug + Copy>(all: &[T], value: &str) -> Option<T> {
    all.iter().copied().find(|t| format!("{t:?}") == value)
}

/// Applies a saved view, skipping anything it doesn't recognize.
pub fn decode(saved: &str, filters: &mut Filters, bucket: &mut Bucket, horizon: &mut Horizon) {
    for (name, value) in saved.lines().filter_map(|l| l.split_once('=')) {
        match name {
            "bucket" => *bucket = by_name(&Bucket::ALL, value).unwrap_or(*bucket),
            "horizon" => *horizon = by_name(&Horizon::ALL, value).unwrap_or(*horizon),
            "region" => {
                if let Some(r) = by_name(&Region::ALL, value) {
                    filters.area = Area::Region(r);
                }
            }
            "place" => {
                if let Some(n) = find_neighborhood(value) {
                    filters.area = Area::Place(n);
                }
            }
            "category" => {
                if let Some(c) = CATEGORIES.iter().find(|c| c.slug == value) {
                    filters.categories.insert(c.slug);
                }
            }
            "tag" if !value.is_empty() => {
                filters.tags.insert(value.to_string());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(saved: &str) -> (Filters, Bucket, Horizon) {
        let (mut filters, mut bucket, mut horizon) =
            (Filters::default(), Bucket::Day, Horizon::Month);
        decode(saved, &mut filters, &mut bucket, &mut horizon);
        (filters, bucket, horizon)
    }

    #[test]
    fn round_trips_everything_but_the_search() {
        let mut filters = Filters {
            search: "trivia".into(),
            area: Area::Place(find_neighborhood("encinitas").unwrap()),
            ..Filters::default()
        };
        filters.categories.insert("chill");
        filters.tags.insert("beginner friendly".into());
        filters.tags.insert("a=b".into());
        let saved = encode(&filters, Bucket::Week, Horizon::TenDays);

        let (restored, bucket, horizon) = decoded(&saved);
        assert_eq!(restored.search, "");
        assert_eq!(restored.area, filters.area);
        assert_eq!(restored.categories, filters.categories);
        assert_eq!(restored.tags, filters.tags);
        assert_eq!((bucket, horizon), (Bucket::Week, Horizon::TenDays));

        let region = Filters {
            area: Area::Region(Region::EastCounty),
            ..Filters::default()
        };
        let (restored, ..) = decoded(&encode(&region, Bucket::Day, Horizon::Month));
        assert_eq!(restored.area, region.area);
    }

    #[test]
    fn unknown_values_are_skipped() {
        let (filters, bucket, horizon) = decoded(
            "bucket=Fortnight\nhorizon=Year\nplace=atlantis\ncategory=loud\ntag=\ngarbage\ntag=trivia",
        );
        assert_eq!(filters.area, Area::All);
        assert!(filters.categories.is_empty());
        assert_eq!(filters.tags.iter().collect::<Vec<_>>(), ["trivia"]);
        assert_eq!((bucket, horizon), (Bucket::Day, Horizon::Year));
    }
}
