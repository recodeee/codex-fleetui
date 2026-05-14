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

    pub fn clear(&mut self) {
        self.modals.clear();
    }

    pub fn active(&self) -> Option<&M> {
        self.modals.last()
    }

    pub fn active_mut(&mut self) -> Option<&mut M> {
        self.modals.last_mut()
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

    #[test]
    fn active_modal_tracks_the_top_of_the_stack() {
        let mut stack = ModalStack::new();
        stack.push("spotlight");
        stack.push("actions");

        assert_eq!(stack.active(), Some(&"actions"));
        assert_eq!(stack.pop(), Some("actions"));
        assert_eq!(stack.active(), Some(&"spotlight"));
    }
}
