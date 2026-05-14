//! Key pattern matching for dashboard input.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::Action;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyPattern {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl KeyPattern {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    pub fn esc() -> Self {
        Self::new(KeyCode::Esc, KeyModifiers::NONE)
    }

    pub fn enter() -> Self {
        Self::new(KeyCode::Enter, KeyModifiers::NONE)
    }

    pub fn tab() -> Self {
        Self::new(KeyCode::Tab, KeyModifiers::NONE)
    }

    pub fn with_modifiers(mut self, modifiers: KeyModifiers) -> Self {
        self.modifiers = modifiers;
        self
    }

    pub fn matches(&self, key: KeyEvent) -> bool {
        self.code == key.code && self.modifiers == key.modifiers
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyBinding {
    pattern: KeyPattern,
    action: Action,
}

impl KeyBinding {
    pub fn new(pattern: KeyPattern, action: Action) -> Self {
        Self { pattern, action }
    }

    pub fn pattern(&self) -> &KeyPattern {
        &self.pattern
    }

    pub fn action(&self) -> &Action {
        &self.action
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Matcher {
    bindings: Vec<KeyBinding>,
}

impl Matcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_bindings(bindings: impl IntoIterator<Item = KeyBinding>) -> Self {
        Self {
            bindings: bindings.into_iter().collect(),
        }
    }

    pub fn bind(mut self, pattern: KeyPattern, action: Action) -> Self {
        self.push(pattern, action);
        self
    }

    pub fn push(&mut self, pattern: KeyPattern, action: Action) {
        self.bindings.push(KeyBinding::new(pattern, action));
    }

    pub fn bindings(&self) -> &[KeyBinding] {
        &self.bindings
    }

    pub fn dispatch(&self, key: KeyEvent) -> Option<Action> {
        self.bindings
            .iter()
            .find(|binding| binding.pattern.matches(key))
            .map(|binding| binding.action.clone())
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{KeyPattern, Matcher};
    use crate::Action;

    fn key(c: char, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), modifiers)
    }

    #[test]
    fn dispatches_first_matching_action() {
        let matcher = Matcher::new()
            .bind(KeyPattern::char('q'), Action::Quit)
            .bind(KeyPattern::char('r'), Action::Refresh);

        assert_eq!(
            matcher.dispatch(key('q', KeyModifiers::NONE)),
            Some(Action::Quit)
        );
        assert_eq!(
            matcher.dispatch(key('r', KeyModifiers::NONE)),
            Some(Action::Refresh)
        );
        assert_eq!(matcher.dispatch(key('x', KeyModifiers::NONE)), None);
    }

    #[test]
    fn modifiers_are_part_of_the_key_pattern() {
        let matcher = Matcher::new().bind(
            KeyPattern::char('r').with_modifiers(KeyModifiers::CONTROL),
            Action::Refresh,
        );

        assert_eq!(matcher.dispatch(key('r', KeyModifiers::NONE)), None);
        assert_eq!(
            matcher.dispatch(key('r', KeyModifiers::CONTROL)),
            Some(Action::Refresh)
        );
    }
}
