//! Generic context-sensitive key dispatch.

use crossterm::event::KeyEvent;

use crate::ContextStack;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyBinding<C, A> {
    context: C,
    key: KeyEvent,
    action: A,
}

impl<C, A> KeyBinding<C, A> {
    pub fn new(context: C, key: KeyEvent, action: A) -> Self {
        Self {
            context,
            key,
            action,
        }
    }

    pub fn context(&self) -> &C {
        &self.context
    }

    pub fn key(&self) -> KeyEvent {
        self.key
    }

    pub fn action(&self) -> &A {
        &self.action
    }

    fn matches_key(&self, key: KeyEvent) -> bool {
        self.key.code == key.code && self.key.modifiers == key.modifiers
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Matcher<C, A> {
    bindings: Vec<KeyBinding<C, A>>,
}

impl<C, A> Default for Matcher<C, A> {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }
}

impl<C, A> Matcher<C, A>
where
    C: Eq,
    A: Clone,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_bindings(bindings: impl IntoIterator<Item = KeyBinding<C, A>>) -> Self {
        Self {
            bindings: bindings.into_iter().collect(),
        }
    }

    pub fn bind(mut self, context: C, key: KeyEvent, action: A) -> Self {
        self.push(context, key, action);
        self
    }

    pub fn push(&mut self, context: C, key: KeyEvent, action: A) {
        self.bindings.push(KeyBinding::new(context, key, action));
    }

    pub fn bindings(&self) -> &[KeyBinding<C, A>] {
        &self.bindings
    }

    pub fn dispatch(&self, stack: &ContextStack<C>, key: KeyEvent) -> Option<A> {
        stack.iter_top_down().find_map(|context| {
            self.bindings
                .iter()
                .find(|binding| binding.context() == context && binding.matches_key(key))
                .map(|binding| binding.action().clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::Matcher;
    use crate::ContextStack;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum TestAction {
        Quit,
        CloseOverlay,
        OpenSpotlight,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum TestContext {
        Root,
        Overlay,
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn dispatch_miss_returns_none() {
        let stack = ContextStack::from_context(TestContext::Root);
        let matcher = Matcher::new().bind(TestContext::Root, key('q'), TestAction::Quit);

        assert_eq!(matcher.dispatch(&stack, key('x')), None);
    }

    #[test]
    fn dispatch_hits_non_top_context_when_top_context_misses() {
        let mut stack = ContextStack::from_context(TestContext::Root);
        stack.push(TestContext::Overlay);
        let matcher = Matcher::new()
            .bind(TestContext::Root, key('s'), TestAction::OpenSpotlight)
            .bind(TestContext::Overlay, key('q'), TestAction::CloseOverlay);

        assert_eq!(
            matcher.dispatch(&stack, key('s')),
            Some(TestAction::OpenSpotlight)
        );
        assert_eq!(
            matcher.dispatch(&stack, key('q')),
            Some(TestAction::CloseOverlay)
        );
    }
}
