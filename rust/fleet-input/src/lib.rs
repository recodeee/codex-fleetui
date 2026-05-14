//! Shared input routing primitives for codex-fleet dashboards.

pub mod action;
pub mod context;
pub mod keymap;
pub mod modal;

pub use action::Action;
pub use context::{ContextLayer, ContextStack};
pub use keymap::{KeyBinding, KeyPattern, Matcher};
pub use modal::ModalStack;
