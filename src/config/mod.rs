//! Typed user and project configuration.

pub mod load;
pub mod merge;
pub mod schema;
pub mod validate;

pub use load::{LoadedConfig, load};
pub use schema::{ProjectPolicy, RawConfig, RawRule, ValidatedConfig, ValidatedRule};
