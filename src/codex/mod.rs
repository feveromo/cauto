//! Native Codex binary discovery, capability catalogs, and launch planning.

pub mod args;
pub mod binary;
pub mod capabilities;
pub mod catalog;
pub mod launch;
pub mod version;

pub use binary::{CodexInstallation, NativeProcessRunner, ProcessRequest, ProcessRunner};
pub use catalog::{CatalogManager, CatalogRequest, ModelCapability, ModelCatalog};
