//! Context-aware key dispatch.

use crossterm::event::KeyEvent;

use crate::{Action, Matcher};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextLayer<C> {
    context: C,
    matcher: Matcher,
}

impl<C> ContextLayer<C> {
    pub fn new(context: C, matcher: Matcher) -> Self {
        Self { context, matcher }
    }

    pub fn context(&self) -> &C {
        &self.context
    }

    pub fn matcher(&self) -> &Matcher {
        &self.matcher
    }

    pub fn matcher_mut(&mut self) -> &mut Matcher {
        &mut self.matcher
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContextStack<C> {
    layers: Vec<ContextLayer<C>>,
}

impl<C> ContextStack<C> {
    pub fn new(context: C, matcher: Matcher) -> Self {
        Self {
            layers: vec![ContextLayer::new(context, matcher)],
        }
    }

    pub fn empty() -> Self {
        Self { layers: Vec::new() }
    }

    pub fn push(&mut self, context: C, matcher: Matcher) {
        self.layers.push(ContextLayer::new(context, matcher));
    }

    pub fn pop(&mut self) -> Option<ContextLayer<C>> {
        if self.layers.len() > 1 {
            self.layers.pop()
        } else {
            None
        }
    }

    pub fn active(&self) -> Option<&ContextLayer<C>> {
        self.layers.last()
    }

    pub fn active_mut(&mut self) -> Option<&mut ContextLayer<C>> {
        self.layers.last_mut()
    }

    pub fn layers(&self) -> &[ContextLayer<C>] {
        &self.layers
    }

    pub fn dispatch(&mut self, key: KeyEvent) -> Option<Action> {
        self.layers
            .iter()
            .rev()
            .find_map(|layer| layer.matcher.dispatch(key))
            .filter(|action| !action.is_noop())
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::ContextStack;
    use crate::{Action, KeyPattern, Matcher};

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum TestContext {
        Root,
        Overlay,
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn top_context_wins_before_root_context() {
        let root = Matcher::new().bind(KeyPattern::char('q'), Action::Quit);
        let overlay = Matcher::new().bind(KeyPattern::char('q'), Action::SelectTab(2));
        let mut stack = ContextStack::new(TestContext::Root, root);

        assert_eq!(stack.dispatch(key('q')), Some(Action::Quit));

        stack.push(TestContext::Overlay, overlay);
        assert_eq!(stack.dispatch(key('q')), Some(Action::SelectTab(2)));
    }

    #[test]
    fn pop_preserves_root_context() {
        let mut stack = ContextStack::new(TestContext::Root, Matcher::new());
        stack.push(TestContext::Overlay, Matcher::new());

        assert_eq!(
            stack.pop().map(|layer| layer.context().clone()),
            Some(TestContext::Overlay)
        );
        assert!(stack.pop().is_none());
        assert_eq!(
            stack.active().map(|layer| layer.context()),
            Some(&TestContext::Root)
        );
    }
}
