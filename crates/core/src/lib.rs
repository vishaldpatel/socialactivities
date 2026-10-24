//! Data model, storage and recurrence logic for San Diego Social Activities.

pub mod catalog;
pub mod db;
pub mod listfile;
pub mod model;
pub mod recurrence;
pub mod store;

pub use listfile::ListFile;
pub use model::{Cost, CostKind, Event, Group, Link, ListInfo, Occurrence};
pub use store::Store;

/// A fresh random record id.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
