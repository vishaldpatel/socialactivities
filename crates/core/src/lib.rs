//! Data model, storage and recurrence logic for San Diego Social Activities.

pub mod catalog;
pub mod db;
pub mod model;
pub mod recurrence;
pub mod seed;
pub mod store;

pub use model::{Cost, CostKind, Event, Group, Link, Occurrence, Source};
pub use store::Store;

/// A fresh random record id.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
