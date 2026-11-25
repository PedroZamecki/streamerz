//! In-memory undo and redo stacks for action authoring and UI edits.
//!
//! The stacks live on the engine's thread (every edit comes through it), so each thread, and each
//! test, has its own.
use crate::engine::Engine;
use crate::model::Action;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

const MAX_HISTORY: usize = 50;

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum UndoItem {
    ActionModified { id: String, before: Action, after: Action },
    ActionDeleted { action: Action },
    ActionCreated { action: Action },
}

#[derive(Default)]
struct Stacks {
    undo: VecDeque<UndoItem>,
    redo: VecDeque<UndoItem>,
}

thread_local! {
    static STACKS: RefCell<Stacks> = RefCell::default();
}

/// Put `item` on top of `stack`, dropping the oldest past the limit.
fn push(stack: &mut VecDeque<UndoItem>, item: UndoItem) {
    stack.push_back(item);
    if stack.len() > MAX_HISTORY {
        stack.pop_front();
    }
}

/// A new edit: it can be undone, and what was undone before can't be redone any more.
fn record(item: UndoItem) {
    STACKS.with_borrow_mut(|stacks| {
        push(&mut stacks.undo, item);
        stacks.redo.clear();
    });
}

/// Clear undo and redo stacks (useful for tests or state resets).
pub fn clear() {
    STACKS.with_borrow_mut(|stacks| {
        stacks.undo.clear();
        stacks.redo.clear();
    });
}

/// Returns whether there are items in the undo stack.
pub fn can_undo() -> bool {
    STACKS.with_borrow(|stacks| !stacks.undo.is_empty())
}

/// Returns whether there are items in the redo stack.
pub fn can_redo() -> bool {
    STACKS.with_borrow(|stacks| !stacks.redo.is_empty())
}

/// Record an action modification before/after snapshot.
pub fn record_action_modified(before: Action, after: Action) {
    if before == after {
        return;
    }
    record(UndoItem::ActionModified {
        id: before.id.clone(),
        before,
        after,
    });
}

/// Record an action deletion.
pub fn record_action_deleted(action: Action) {
    record(UndoItem::ActionDeleted { action });
}

/// Record a new action creation.
pub fn record_action_created(action: Action) {
    record(UndoItem::ActionCreated { action });
}

/// Put `action` back: in its place when one with its id is there (so an id is never listed twice),
/// else at the end.
fn restore(engine: &Rc<Engine>, action: &Action) {
    let action = action.clone();
    engine.change(
        |config| match config.actions.actions.iter_mut().find(|a| a.id == action.id) {
            Some(target) => *target = action,
            None => config.actions.actions.push(action),
        },
    );
}

fn remove(engine: &Rc<Engine>, id: &str) {
    engine.change(|config| config.actions.actions.retain(|a| a.id != id));
}

/// Set an action that still exists to `state`; false when it is gone.
fn set(engine: &Rc<Engine>, id: &str, state: &Action) -> bool {
    if !engine.config().actions.actions.iter().any(|a| a.id == id) {
        return false;
    }
    restore(engine, state);
    true
}

/// Apply `item` backwards (undo) or forwards (redo). An edit of an action that is gone since can't
/// be applied: it is dropped and the error says so.
fn apply(engine: &Rc<Engine>, item: &UndoItem, undo: bool) -> Result<String, String> {
    match (item, undo) {
        (UndoItem::ActionModified { id, before, after }, _) => {
            if set(engine, id, if undo { before } else { after }) {
                Ok(id.clone())
            } else {
                Err(crate::i18n::tr("actions.item_gone").into())
            }
        }
        (UndoItem::ActionDeleted { action }, true) | (UndoItem::ActionCreated { action }, false) => {
            restore(engine, action);
            Ok(action.id.clone())
        }
        (UndoItem::ActionDeleted { action }, false) | (UndoItem::ActionCreated { action }, true) => {
            remove(engine, &action.id);
            Ok(action.id.clone())
        }
    }
}

/// Take the top of one stack, apply it, and move it to the other one.
fn step(engine: &Rc<Engine>, undo: bool) -> Result<Option<String>, String> {
    let item = STACKS.with_borrow_mut(|stacks| {
        if undo {
            stacks.undo.pop_back()
        } else {
            stacks.redo.pop_back()
        }
    });
    let Some(item) = item else {
        return Ok(None);
    };
    let id = apply(engine, &item, undo)?;
    STACKS.with_borrow_mut(|stacks| push(if undo { &mut stacks.redo } else { &mut stacks.undo }, item));
    Ok(Some(id))
}

/// Undo the last recorded operation.
pub fn undo(engine: &Rc<Engine>) -> Result<Option<String>, String> {
    step(engine, true)
}

/// Redo the last undone operation.
pub fn redo(engine: &Rc<Engine>) -> Result<Option<String>, String> {
    step(engine, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn action(id: &str, name: &str) -> Action {
        Action {
            id: id.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    fn names(engine: &Rc<Engine>) -> Vec<(String, String)> {
        engine
            .config()
            .actions
            .actions
            .iter()
            .map(|a| (a.id.clone(), a.name.clone()))
            .collect()
    }

    fn undo_len() -> usize {
        STACKS.with_borrow(|stacks| stacks.undo.len())
    }

    #[test]
    fn record_and_stack_capacity() {
        clear();
        assert!(!can_undo());
        assert!(!can_redo());
        let a1 = action("a1", "First");
        let a2 = action("a1", "Second");
        record_action_modified(a1.clone(), a2.clone());
        assert!(can_undo());
        assert!(!can_redo());
        // Pushing identical state should not create undo entries
        record_action_modified(a2.clone(), a2.clone());
        assert_eq!(undo_len(), 1);
        // Cap at MAX_HISTORY
        for i in 0..60 {
            record_action_modified(
                action("a1", &format!("Name {i}")),
                action("a1", &format!("Name {}", i + 1)),
            );
        }
        assert_eq!(undo_len(), MAX_HISTORY);
        // The oldest go first: the bottom of the stack is the eleventh of those edits.
        let bottom = STACKS.with_borrow(|stacks| stacks.undo.front().cloned());
        assert!(matches!(bottom, Some(UndoItem::ActionModified { before, .. }) if before.name == "Name 10"));
        clear();
        assert!(!can_undo());
    }

    #[test]
    fn stacks_belong_to_their_thread() {
        clear();
        record_action_created(action("here", "Here"));
        let elsewhere = std::thread::spawn(|| {
            let before = can_undo();
            record_action_created(action("there", "There"));
            (before, undo_len())
        })
        .join()
        .unwrap();
        assert_eq!(elsewhere, (false, 1));
        assert_eq!(undo_len(), 1);
        clear();
    }

    /// Every sequence of edits, undone all the way, gives back the first state; redone all the way,
    /// the last one. Undo and redo of each step land on the state of the step before or after it.
    #[test]
    fn undo_and_redo_walk_back_and_forth_through_every_state() {
        crate::engine::tests::run(async {
            clear();
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let mut states = vec![names(&engine)];
            // A fixed sequence that creates, renames, deletes and creates again.
            let ops: [(&str, &str, &str); 8] = [
                ("create", "a", "A"),
                ("create", "b", "B"),
                ("rename", "a", "A2"),
                ("delete", "b", ""),
                ("create", "c", "C"),
                ("rename", "c", "C2"),
                ("rename", "a", "A3"),
                ("delete", "a", ""),
            ];
            for (op, id, name) in ops {
                match op {
                    "create" => {
                        let made = action(id, name);
                        record_action_created(made.clone());
                        engine.change(|config| config.actions.actions.push(made));
                    }
                    "rename" => {
                        let before = engine
                            .config()
                            .actions
                            .actions
                            .iter()
                            .find(|a| a.id == id)
                            .cloned()
                            .unwrap();
                        let mut after = before.clone();
                        after.name = name.into();
                        record_action_modified(before, after.clone());
                        restore(&engine, &after);
                    }
                    _ => {
                        let gone = engine
                            .config()
                            .actions
                            .actions
                            .iter()
                            .find(|a| a.id == id)
                            .cloned()
                            .unwrap();
                        record_action_deleted(gone);
                        remove(&engine, id);
                    }
                }
                states.push(names(&engine));
            }
            let sorted = |mut list: Vec<(String, String)>| {
                list.sort();
                list
            };
            for at in (0..ops.len()).rev() {
                assert!(undo(&engine).unwrap().is_some());
                assert_eq!(sorted(names(&engine)), sorted(states[at].clone()), "undo to {at}");
            }
            assert_eq!(undo(&engine).unwrap(), None, "nothing left to undo");
            for (at, state) in states.iter().enumerate().skip(1) {
                assert!(redo(&engine).unwrap().is_some());
                assert_eq!(sorted(names(&engine)), sorted(state.clone()), "redo to {at}");
            }
            assert_eq!(redo(&engine).unwrap(), None, "nothing left to redo");
            clear();
        });
    }

    #[test]
    fn a_new_edit_after_an_undo_drops_the_redo() {
        crate::engine::tests::run(async {
            clear();
            let engine = crate::engine::tests::engine(json!({"actions": []}));
            let first = action("a", "A");
            record_action_created(first.clone());
            engine.change(|config| config.actions.actions.push(first.clone()));
            undo(&engine).unwrap();
            assert!(can_redo());
            record_action_created(action("b", "B"));
            assert!(!can_redo());
            assert_eq!(redo(&engine).unwrap(), None);
            clear();
        });
    }

    #[test]
    fn restoring_a_deleted_action_never_lists_its_id_twice() {
        crate::engine::tests::run(async {
            clear();
            let engine = crate::engine::tests::engine(json!({"actions": [{"id": "a", "name": "Old"}]}));
            let old = engine.config().actions.actions[0].clone();
            record_action_deleted(old);
            remove(&engine, "a");
            // Imported again meanwhile (an edit the stacks don't know about).
            restore(&engine, &action("a", "Imported"));
            assert_eq!(undo(&engine).unwrap().as_deref(), Some("a"));
            assert_eq!(names(&engine), vec![("a".to_owned(), "Old".to_owned())]);
            clear();
        });
    }

    #[test]
    fn an_edit_of_an_action_gone_since_is_an_error_and_is_dropped() {
        crate::engine::tests::run(async {
            clear();
            let engine = crate::engine::tests::engine(json!({"actions": [{"id": "a", "name": "A"}]}));
            record_action_created(action("x", "X"));
            record_action_modified(action("a", "A"), action("a", "A2"));
            // Gone by a path the stacks don't know about (a group deleted, say).
            remove(&engine, "a");
            assert!(undo(&engine).is_err());
            assert!(names(&engine).is_empty(), "nothing is brought back");
            assert!(!can_redo(), "the edit that couldn't be undone isn't offered to redo");
            // The one under it is next.
            assert_eq!(undo(&engine).unwrap().as_deref(), Some("x"));
            clear();
        });
    }
}
