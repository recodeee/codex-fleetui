//! Dashboard actions produced by keyboard and mouse input.

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum Action {
    /// No state transition should occur.
    #[default]
    Noop,
    /// End the current dashboard event loop.
    Quit,
    /// Refresh the dashboard data immediately.
    Refresh,
    /// Select a dashboard tab/window by numeric index.
    SelectTab(usize),
}

impl Action {
    pub fn is_noop(&self) -> bool {
        matches!(self, Self::Noop)
    }
}

#[cfg(test)]
mod tests {
    use super::Action;

    #[test]
    fn default_action_is_noop() {
        assert_eq!(Action::default(), Action::Noop);
        assert!(Action::default().is_noop());
    }
}
