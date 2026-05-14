//! Focus-context stack used by key dispatch.

use crossterm::event::KeyEvent;

use crate::Matcher;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContextStack<C> {
    contexts: Vec<C>,
}

impl<C> ContextStack<C> {
    pub fn new() -> Self {
        Self {
            contexts: Vec::new(),
        }
    }

    pub fn from_context(context: C) -> Self {
        Self {
            contexts: vec![context],
        }
    }

    pub fn push(&mut self, context: C) {
        self.contexts.push(context);
    }

    pub fn pop(&mut self) -> Option<C> {
        self.contexts.pop()
    }

    pub fn top(&self) -> Option<&C> {
        self.contexts.last()
    }

    pub fn top_mut(&mut self) -> Option<&mut C> {
        self.contexts.last_mut()
    }

    pub fn len(&self) -> usize {
        self.contexts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }

    pub fn iter_top_down(&self) -> impl Iterator<Item = &C> {
        self.contexts.iter().rev()
    }
}

impl<C> ContextStack<C>
where
    C: Eq,
{
    pub fn dispatch<A>(&self, matcher: &Matcher<C, A>, key: KeyEvent) -> Option<A>
    where
        A: Clone,
    {
        matcher.dispatch(self, key)
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::ContextStack;
    use crate::Matcher;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum TestContext {
        Root,
        Overlay,
    }

    #[test]
    fn push_pop_on_empty_context_stack() {
        let mut stack = ContextStack::new();

        assert_eq!(stack.pop(), None);
        assert_eq!(stack.top(), None);

        stack.push(TestContext::Root);
        stack.push(TestContext::Overlay);

        assert_eq!(stack.top(), Some(&TestContext::Overlay));
        assert_eq!(stack.pop(), Some(TestContext::Overlay));
        assert_eq!(stack.pop(), Some(TestContext::Root));
        assert!(stack.is_empty());
    }

    #[test]
    fn dispatch_delegates_through_matcher_top_down() {
        let mut stack = ContextStack::from_context(TestContext::Root);
        stack.push(TestContext::Overlay);
        let matcher = Matcher::new().bind(
            TestContext::Root,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            "quit",
        );

        assert_eq!(
            stack.dispatch(
                &matcher,
                KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
            ),
            Some("quit")
        );
    }
}
