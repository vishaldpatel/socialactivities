//! Fixed vocabularies shared by the defaults and user databases.
//!
//! These live in code rather than in a table so records in either database
//! can reference them by slug without cross-database joins.

/// A broad part of San Diego County that neighborhoods and cities roll up into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    NorthCounty,
    SanDiegoCity,
    EastCounty,
    SouthCounty,
}

impl Region {
    pub const ALL: [Region; 4] = [
        Region::NorthCounty,
        Region::SanDiegoCity,
        Region::EastCounty,
        Region::SouthCounty,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Region::NorthCounty => "North County",
            Region::SanDiegoCity => "San Diego City",
            Region::EastCounty => "East County SD",
            Region::SouthCounty => "South County SD",
        }
    }

    pub fn neighborhoods(self) -> impl Iterator<Item = Neighborhood> {
        NEIGHBORHOODS
            .iter()
            .copied()
            .filter(move |n| n.region == Some(self))
    }
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A neighborhood, area or city of San Diego County, used for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Neighborhood {
    pub slug: &'static str,
    pub name: &'static str,
    /// `None` for places that span the county.
    pub region: Option<Region>,
}

/// The slug for records that aren't tied to one place.
pub const COUNTYWIDE: &str = "countywide";

const fn n(slug: &'static str, name: &'static str, region: Region) -> Neighborhood {
    Neighborhood {
        slug,
        name,
        region: Some(region),
    }
}

use Region::{EastCounty as E, NorthCounty as N, SanDiegoCity as C, SouthCounty as S};

/// Grouped by region, then alphabetical.
pub const NEIGHBORHOODS: &[Neighborhood] = &[
    Neighborhood {
        slug: COUNTYWIDE,
        name: "Countywide / Various",
        region: None,
    },
    n("carlsbad", "Carlsbad", N),
    n("del-mar", "Del Mar", N),
    n("encinitas", "Encinitas", N),
    n("escondido", "Escondido", N),
    n("fallbrook", "Fallbrook", N),
    n("oceanside", "Oceanside", N),
    n("poway", "Poway", N),
    n("ramona", "Ramona", N),
    n("san-marcos", "San Marcos", N),
    n("solana-beach", "Solana Beach", N),
    n("vista", "Vista", N),
    n("balboa-park", "Balboa Park", C),
    n("clairemont", "Clairemont", C),
    n("downtown", "Downtown", C),
    n("golden-hill", "Golden Hill", C),
    n("hillcrest", "Hillcrest", C),
    n("kearny-mesa", "Kearny Mesa", C),
    n("la-jolla", "La Jolla", C),
    n("little-italy", "Little Italy", C),
    n("mira-mesa", "Mira Mesa", C),
    n("mission-bay", "Mission Bay", C),
    n("mission-beach", "Mission Beach", C),
    n("mission-valley", "Mission Valley", C),
    n("north-park", "North Park", C),
    n("ocean-beach", "Ocean Beach", C),
    n("pacific-beach", "Pacific Beach", C),
    n("point-loma", "Point Loma", C),
    n("rancho-bernardo", "Rancho Bernardo", C),
    n("south-park", "South Park", C),
    n("university-city", "University City", C),
    n("alpine", "Alpine", E),
    n("el-cajon", "El Cajon", E),
    n("julian", "Julian", E),
    n("la-mesa", "La Mesa", E),
    n("lakeside", "Lakeside", E),
    n("lemon-grove", "Lemon Grove", E),
    n("santee", "Santee", E),
    n("spring-valley", "Spring Valley", E),
    n("chula-vista", "Chula Vista", S),
    n("coronado", "Coronado", S),
    n("imperial-beach", "Imperial Beach", S),
    n("national-city", "National City", S),
    n("otay-mesa", "Otay Mesa", S),
];

impl std::fmt::Display for Neighborhood {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

pub fn find_neighborhood(slug: &str) -> Option<Neighborhood> {
    NEIGHBORHOODS.iter().copied().find(|n| n.slug == slug)
}

/// Display name for a neighborhood slug, falling back to the slug itself.
pub fn neighborhood_name(slug: &str) -> &str {
    find_neighborhood(slug).map_or(slug, |n| n.name)
}

/// The fixed activity categories. Groups and events can carry several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Category {
    pub slug: &'static str,
    pub name: &'static str,
}

const fn c(slug: &'static str, name: &'static str) -> Category {
    Category { slug, name }
}

pub const CATEGORIES: &[Category] = &[c("active", "Shake 'n Bake"), c("chill", "Chill")];

pub fn category_name(slug: &str) -> &str {
    CATEGORIES
        .iter()
        .find(|c| c.slug == slug)
        .map_or(slug, |c| c.name)
}

/// Normalizes a free-form tag: trimmed, lowercase, single spaces. `None` if empty.
pub fn normalize_tag(tag: &str) -> Option<String> {
    let tag = tag
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    (!tag.is_empty()).then_some(tag)
}

/// Kinds of links a group can list, in display order.
pub const LINK_KINDS: &[&str] = &[
    "website",
    "meetup",
    "instagram",
    "facebook",
    "discord",
    "other",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_neighborhood_but_countywide_has_a_region() {
        for n in NEIGHBORHOODS {
            assert_eq!(n.region.is_none(), n.slug == COUNTYWIDE, "{}", n.slug);
        }
        assert_eq!(
            find_neighborhood("encinitas").and_then(|n| n.region),
            Some(Region::NorthCounty)
        );
    }

    #[test]
    fn slugs_are_unique() {
        let mut slugs: Vec<_> = NEIGHBORHOODS.iter().map(|n| n.slug).collect();
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), NEIGHBORHOODS.len());
    }

    #[test]
    fn normalizes_tags() {
        assert_eq!(
            normalize_tag("  Dog   Friendly "),
            Some("dog friendly".into())
        );
        assert_eq!(normalize_tag("   "), None);
    }
}
