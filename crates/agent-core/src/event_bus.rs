//! Simple event bus for decoupled communication between agent runtime and UI.
//!
//! The bus is single-threaded (WASM constraint) and uses interior mutability
//! via RefCell. Events are buffered and drained by the UI on each frame.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use agent_types::event::AgentEvent;

/// Shared event bus — clone-cheap via Rc.
#[derive(Clone)]
pub struct EventBus {
    inner: Rc<RefCell<VecDeque<AgentEvent>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    /// Publish an event. Called by the agent runtime.
    pub fn emit(&self, event: AgentEvent) {
        self.inner.borrow_mut().push_back(event);
    }

    /// Drain all pending events. Called by the UI layer each frame.
    pub fn drain(&self) -> Vec<AgentEvent> {
        self.inner.borrow_mut().drain(..).collect()
    }

    /// Check if there are pending events (useful for egui repaint triggers).
    pub fn has_pending(&self) -> bool {
        !self.inner.borrow().is_empty()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
