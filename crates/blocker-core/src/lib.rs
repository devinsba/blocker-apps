//! Platform-independent blocking logic shared by the CLI, the app agent and
//! the Network Extension content filter.
//!
//! Nothing in this crate touches the network stack: it decides, the platform
//! layers enforce.

pub mod config;
pub mod domain;
pub mod domainset;
pub mod engine;
pub mod guard;
pub mod lists;
pub mod manifest;
pub mod schedule;
pub mod sni;
pub mod status;
pub mod store;

pub use config::Config;
pub use domainset::DomainSet;
pub use engine::{Engine, FlowInfo, Proto, Verdict, WeekTime};
pub use store::Store;
