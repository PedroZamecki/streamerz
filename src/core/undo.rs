//! In-memory undo and redo stacks for action authoring and UI edits.
use crate::engine::Engine;
use crate::model::Action;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Mutex;

const MAX_HISTORY: usize = 50;

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum UndoItem {
    ActionModified { id: String, before: Action, after: Action },
    ActionDeleted { action: Action },
    ActionCreated { action: Action },
}

static UNDO_STACK: Mutex<VecDeque<UndoItem>> = Mutex::new(VecDeque::new());
static REDO_STACK: Mutex<VecDeque<UndoItem>> = Mutex::new(VecDeque::new());

/// Clear undo and redo stacks (useful for tests or state resets).
pub fn clear() {
    let mut undo = UNDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
    let mut redo = REDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
    undo.clear();
    redo.clear();
}

/// Returns whether there are items in the undo stack.
pub fn can_undo() -> bool {
    let undo = UNDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
    !undo.is_empty()
}

/// Returns whether there are items in the redo stack.
pub fn can_redo() -> bool {
    let redo = REDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
    !redo.is_empty()
}

/// Record an action modification before/after snapshot.
pub fn record_action_modified(before: Action, after: Action) {
    if before == after {
        return;
    }
    let mut undo = UNDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
    let mut redo = REDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
    undo.push_back(UndoItem::ActionModified {
        id: before.id.clone(),
        before,
        after,
    });
    if undo.len() > MAX_HISTORY {
        undo.pop_front();
    }
    redo.clear();
}

/// Record an action deletion.
pub fn record_action_deleted(action: Action) {
    let mut undo = UNDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
    let mut redo = REDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
    undo.push_back(UndoItem::ActionDeleted { action });
    if undo.len() > MAX_HISTORY {
        undo.pop_front();
    }
    redo.clear();
}

/// Record a new action creation.
pub fn record_action_created(action: Action) {
    let mut undo = UNDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
    let mut redo = REDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
    undo.push_back(UndoItem::ActionCreated { action });
    if undo.len() > MAX_HISTORY {
        undo.pop_front();
    }
    redo.clear();
}

/// Undo the last recorded operation.
pub fn undo(engine: &Rc<Engine>) -> Result<Option<String>, String> {
    let item = {
        let mut undo = UNDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
        undo.pop_back()
    };
    let Some(item) = item else {
        return Ok(None);
    };

    let (action_id, redo_item) = match &item {
        UndoItem::ActionModified { id, before, after } => {
            let before_clone = before.clone();
            engine.change(|config| {
                if let Some(target) = config.actions.actions.iter_mut().find(|a| a.id == *id) {
                    *target = before_clone;
                }
            });
            (
                id.clone(),
                Some(UndoItem::ActionModified {
                    id: id.clone(),
                    before: before.clone(),
                    after: after.clone(),
                }),
            )
        }
        UndoItem::ActionDeleted { action } => {
            let action_clone = action.clone();
            engine.change(|config| {
                config.actions.actions.push(action_clone);
            });
            (
                action.id.clone(),
                Some(UndoItem::ActionDeleted { action: action.clone() }),
            )
        }
        UndoItem::ActionCreated { action } => {
            let id = action.id.clone();
            engine.change(|config| {
                config.actions.actions.retain(|a| a.id != id);
            });
            (
                action.id.clone(),
                Some(UndoItem::ActionCreated { action: action.clone() }),
            )
        }
    };

    if let Some(redo_item) = redo_item {
        let mut redo = REDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
        redo.push_back(redo_item);
        if redo.len() > MAX_HISTORY {
            redo.pop_front();
        }
    }

    Ok(Some(action_id))
}

/// Redo the last undone operation.
pub fn redo(engine: &Rc<Engine>) -> Result<Option<String>, String> {
    let item = {
        let mut redo = REDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
        redo.pop_back()
    };
    let Some(item) = item else {
        return Ok(None);
    };

    let (action_id, undo_item) = match &item {
        UndoItem::ActionModified { id, before, after } => {
            let after_clone = after.clone();
            engine.change(|config| {
                if let Some(target) = config.actions.actions.iter_mut().find(|a| a.id == *id) {
                    *target = after_clone;
                }
            });
            (
                id.clone(),
                Some(UndoItem::ActionModified {
                    id: id.clone(),
                    before: before.clone(),
                    after: after.clone(),
                }),
            )
        }
        UndoItem::ActionDeleted { action } => {
            let id = action.id.clone();
            engine.change(|config| {
                config.actions.actions.retain(|a| a.id != id);
            });
            (
                action.id.clone(),
                Some(UndoItem::ActionDeleted { action: action.clone() }),
            )
        }
        UndoItem::ActionCreated { action } => {
            let action_clone = action.clone();
            engine.change(|config| {
                config.actions.actions.push(action_clone);
            });
            (
                action.id.clone(),
                Some(UndoItem::ActionCreated { action: action.clone() }),
            )
        }
    };

    if let Some(undo_item) = undo_item {
        let mut undo = UNDO_STACK.lock().unwrap_or_else(|e| e.into_inner());
        undo.push_back(undo_item);
        if undo.len() > MAX_HISTORY {
            undo.pop_front();
        }
    }

    Ok(Some(action_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_stack_capacity() {
        clear();
        assert!(!can_undo());
        assert!(!can_redo());

        let a1 = Action {
            id: "a1".into(),
            name: "First".into(),
            ..Default::default()
        };

        let mut a2 = a1.clone();
        a2.name = "Second".into();

        record_action_modified(a1.clone(), a2.clone());
        assert!(can_undo());
        assert!(!can_redo());

        // Pushing identical state should not create undo entries
        record_action_modified(a2.clone(), a2.clone());
        {
            let undo = UNDO_STACK.lock().unwrap();
            assert_eq!(undo.len(), 1);
        }

        // Cap at MAX_HISTORY
        for i in 0..60 {
            let mut from = a1.clone();
            from.name = format!("Name {i}");
            let mut to = from.clone();
            to.name = format!("Name {}", i + 1);
            record_action_modified(from, to);
        }
        {
            let undo = UNDO_STACK.lock().unwrap();
            assert_eq!(undo.len(), MAX_HISTORY);
        }

        clear();
        assert!(!can_undo());
    }
}
