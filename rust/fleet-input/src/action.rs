//! Dashboard actions produced by input routing.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    CloseOverlay,
    FocusNextTab,
    FocusPrevTab,
    Quit,
    OpenSpotlight,
}

#[cfg(test)]
mod tests {
    use super::Action;

    #[test]
    fn actions_are_small_copyable_commands() {
        let action = Action::OpenSpotlight;

        assert_eq!(action, Action::OpenSpotlight);
        assert_ne!(action, Action::CloseOverlay);
    }
}
