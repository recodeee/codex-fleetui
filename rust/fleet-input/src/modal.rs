//! Generic stack for transient modal state.

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModalStack<M> {
    modals: Vec<M>,
}

impl<M> ModalStack<M> {
    pub fn new() -> Self {
        Self { modals: Vec::new() }
    }

    pub fn from_modal(modal: M) -> Self {
        Self {
            modals: vec![modal],
        }
    }

    pub fn push(&mut self, modal: M) {
        self.modals.push(modal);
    }

    pub fn pop(&mut self) -> Option<M> {
        self.modals.pop()
    }

    pub fn top(&self) -> Option<&M> {
        self.modals.last()
    }

    pub fn top_mut(&mut self) -> Option<&mut M> {
        self.modals.last_mut()
    }

    pub fn clear(&mut self) {
        self.modals.clear();
    }

    pub fn len(&self) -> usize {
        self.modals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modals.is_empty()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &M> {
        self.modals.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::ModalStack;

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum TestModal {
        ContextMenu { selected: usize },
        Spotlight,
    }

    #[test]
    fn modal_stacked_on_modal_keeps_top_mutable() {
        let mut stack = ModalStack::from_modal(TestModal::ContextMenu { selected: 0 });
        stack.push(TestModal::Spotlight);

        assert_eq!(stack.top(), Some(&TestModal::Spotlight));
        assert_eq!(stack.pop(), Some(TestModal::Spotlight));

        if let Some(TestModal::ContextMenu { selected }) = stack.top_mut() {
            *selected = 1;
        }

        assert_eq!(stack.top(), Some(&TestModal::ContextMenu { selected: 1 }));
    }
}
