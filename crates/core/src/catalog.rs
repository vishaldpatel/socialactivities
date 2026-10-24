//! Fixed vocabularies shared by the defaults and user databases.
//!
//! These live in code rather than in a table so records in either database
//! can reference them by slug without cross-database joins.

/// A neighborhood or area of San Diego County, used for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Neighborhood {
    pub slug: &'static str,
    pub name: &'static str,
}

const fn n(slug: &'static str, name: &'static str) -> Neighborhood {
    Neighborhood { slug, name }
}

pub const NEIGHBORHOODS: &[Neighborhood] = &[
    n("countywide", "Countywide / Various"),
    n("alpine", "Alpine"),
    n("balboa-park", "Balboa Park"),
    n("carlsbad", "Carlsbad"),
    n("chula-vista", "Chula Vista"),
    n("clairemont", "Clairemont"),
    n("coronado", "Coronado"),
    n("del-mar", "Del Mar"),
    n("downtown", "Downtown"),
    n("el-cajon", "El Cajon"),
    n("encinitas", "Encinitas"),
    n("escondido", "Escondido"),
    n("golden-hill", "Golden Hill"),
    n("hillcrest", "Hillcrest"),
    n("imperial-beach", "Imperial Beach"),
    n("julian", "Julian"),
    n("kearny-mesa", "Kearny Mesa"),
    n("la-jolla", "La Jolla"),
    n("la-mesa", "La Mesa"),
    n("little-italy", "Little Italy"),
    n("mira-mesa", "Mira Mesa"),
    n("mission-bay", "Mission Bay"),
    n("mission-beach", "Mission Beach"),
    n("mission-valley", "Mission Valley"),
    n("national-city", "National City"),
    n("north-park", "North Park"),
    n("ocean-beach", "Ocean Beach"),
    n("oceanside", "Oceanside"),
    n("pacific-beach", "Pacific Beach"),
    n("point-loma", "Point Loma"),
    n("poway", "Poway"),
    n("ramona", "Ramona"),
    n("rancho-bernardo", "Rancho Bernardo"),
    n("santee", "Santee"),
    n("south-park", "South Park"),
    n("university-city", "University City"),
];

impl std::fmt::Display for Neighborhood {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

/// Display name for a neighborhood slug, falling back to the slug itself.
pub fn neighborhood_name(slug: &str) -> &str {
    NEIGHBORHOODS
        .iter()
        .find(|n| n.slug == slug)
        .map_or(slug, |n| n.name)
}

/// An activity category. Groups can carry several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub slug: &'static str,
    pub name: &'static str,
}

const fn t(slug: &'static str, name: &'static str) -> Tag {
    Tag { slug, name }
}

pub const TAGS: &[Tag] = &[
    t("outdoors", "Outdoors"),
    t("fitness", "Fitness"),
    t("water", "Water Sports"),
    t("games", "Games"),
    t("trivia", "Trivia"),
    t("dance", "Dance"),
    t("language", "Language"),
    t("professional", "Networking"),
    t("volunteering", "Volunteering"),
    t("creative", "Creative"),
    t("festival", "Festival"),
    t("sky", "Sky Watching"),
];

pub fn tag_name(slug: &str) -> &str {
    TAGS.iter()
        .find(|t| t.slug == slug)
        .map_or(slug, |t| t.name)
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
