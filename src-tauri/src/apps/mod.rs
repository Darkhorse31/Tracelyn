pub mod discover;
pub mod merge;
pub mod parsers;
pub mod store;
pub mod types;

pub use discover::discover_apps;
pub use store::{load_manual_apps, migrate_app_registry, set_app_hidden, upsert_manual_app};
pub use types::TracelynApp;
