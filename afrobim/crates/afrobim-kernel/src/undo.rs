/// Snapshot undo/redo. The owner hands over the whole state after each edit; undo and redo
/// return the state to restore. A commit equal to the current state is dropped, so undo never
/// appears to do nothing.
#[derive(Debug, Clone)]
pub struct UndoHistory<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    current: T,
    limit: usize,
}

impl<T: Clone + PartialEq> UndoHistory<T> {
    pub fn new(initial: T, limit: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), current: initial, limit: limit.max(1) }
    }

    pub fn current(&self) -> &T {
        &self.current
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Records `state` as the new current state and clears redo. Returns false for a no-op.
    pub fn commit(&mut self, state: T) -> bool {
        if state == self.current {
            return false;
        }
        self.undo.push(std::mem::replace(&mut self.current, state));
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
        true
    }

    /// Forgets all history and starts from `state` (new or opened document).
    pub fn reset(&mut self, state: T) {
        self.undo.clear();
        self.redo.clear();
        self.current = state;
    }

    pub fn undo(&mut self) -> Option<&T> {
        let previous = self.undo.pop()?;
        self.redo.push(std::mem::replace(&mut self.current, previous));
        Some(&self.current)
    }

    pub fn redo(&mut self) -> Option<&T> {
        let next = self.redo.pop()?;
        self.undo.push(std::mem::replace(&mut self.current, next));
        Some(&self.current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_and_no_op_commits() {
        let mut h = UndoHistory::new("a", 100);
        assert!(!h.can_undo());
        assert!(h.commit("b"));
        assert!(!h.commit("b"));
        assert!(h.commit("c"));
        assert_eq!(h.undo(), Some(&"b"));
        assert_eq!(h.redo(), Some(&"c"));
        h.undo();
        h.commit("d"); // a new edit clears redo
        assert!(!h.can_redo());
        assert_eq!(h.undo(), Some(&"b"));
        assert_eq!(h.undo(), Some(&"a"));
        assert_eq!(h.undo(), None);
    }

    #[test]
    fn history_is_bounded_and_keeps_the_newest() {
        let mut h = UndoHistory::new(0, 3);
        for i in 1..=10 {
            h.commit(i);
        }
        let mut seen = Vec::new();
        while let Some(s) = h.undo() {
            seen.push(*s);
        }
        assert_eq!(seen, vec![9, 8, 7]);
    }
}
