//! Shared input routing primitives for codex-fleet dashboards.

pub mod action;
pub mod context;
pub mod keymap;
pub mod modal;

pub use action::Action;
pub use context::ContextStack;
pub use keymap::{KeyBinding, Matcher};
pub use modal::ModalStack;
