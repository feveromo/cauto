//! Versioned digest-checked cache support.

pub mod atomic;
pub mod envelope;

pub use atomic::{atomic_write, ensure_private_dir};
pub use envelope::CacheEnvelope;
