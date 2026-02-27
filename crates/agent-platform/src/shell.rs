//! Shell adapter — bridges to Wasmer-JS running in a Web Worker.
//!
//! Architecture:
//! - Main thread (egui) ←→ Web Worker (Wasmer-JS + WASIX bash)
//! - Communication via postMessage with JSON-serialized WorkerCommand/WorkerEvent
//! - The Worker loads the Wasmer-JS SDK and spawns WASIX bash processes

use std::cell::RefCell;
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::Rc;

use async_trait::async_trait;
use futures::channel::oneshot;
use futures::stream::{self, Stream};
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, Worker};

use agent_core::ports::{ShellPort, ShellStreamEvent};
use agent_types::{
    AgentError, Result,
    event::{WorkerCommand, WorkerEvent},
    tool::{ExecHandle, ExecResult},
};

/// Shell adapter that communicates with Wasmer-JS via a Web Worker.
pub struct WasmerShellAdapter {
    worker: Worker,
    ready: RefCell<bool>,
    next_id: RefCell<u64>,
    /// Pending one-shot results, keyed by execution ID
    pending: Rc<RefCell<HashMap<u64, PendingExec>>>,
}

struct PendingExec {
    stdout: String,
    stderr: String,
    sender: Option<oneshot::Sender<ExecResult>>,
}

impl WasmerShellAdapter {
    /// Create a new shell adapter. Spawns the Web Worker.
    pub fn new() -> Result<Self> {
        // Create the worker from the bundled JS file
        let worker = Worker::new("./worker.js")
            .map_err(|e| AgentError::Shell(format!("Failed to create worker: {:?}", e)))?;

        let pending: Rc<RefCell<HashMap<u64, PendingExec>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let ready = Rc::new(RefCell::new(false));

        // Set up message handler for worker events
        let pending_clone = pending.clone();
        let ready_clone = ready.clone();
        let onmessage = Closure::wrap(Box::new(move |event: MessageEvent| {
            let data = event.data();
            if let Ok(json_str) = js_sys::JSON::stringify(&data) {
                let s: String = json_str.into();
                if let Ok(worker_event) = serde_json::from_str::<WorkerEvent>(&s) {
                    match worker_event {
                        WorkerEvent::Ready => {
                            *ready_clone.borrow_mut() = true;
                            log::info!("Wasmer-JS worker ready");
                        }
                        WorkerEvent::Stdout { id, data } => {
                            if let Some(exec) = pending_clone.borrow_mut().get_mut(&id) {
                                exec.stdout.push_str(&data);
                            }
                        }
                        WorkerEvent::Stderr { id, data } => {
                            if let Some(exec) = pending_clone.borrow_mut().get_mut(&id) {
                                exec.stderr.push_str(&data);
                            }
                        }
                        WorkerEvent::ExitCode { id, code } => {
                            if let Some(mut exec) = pending_clone.borrow_mut().remove(&id) {
                                if let Some(sender) = exec.sender.take() {
                                    let _ = sender.send(ExecResult {
                                        stdout: exec.stdout,
                                        stderr: exec.stderr,
                                        exit_code: code,
                                    });
                                }
                            }
                        }
                        WorkerEvent::Error { id, message } => {
                            if let Some(mut exec) = pending_clone.borrow_mut().remove(&id) {
                                exec.stderr.push_str(&message);
                                if let Some(sender) = exec.sender.take() {
                                    let _ = sender.send(ExecResult {
                                        stdout: exec.stdout,
                                        stderr: exec.stderr,
                                        exit_code: 1,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }) as Box<dyn FnMut(MessageEvent)>);

        worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        // Send init command to the worker
        let init_cmd = serde_json::to_string(&WorkerCommand::Init).unwrap();
        let js_val = js_sys::JSON::parse(&init_cmd).unwrap();
        worker
            .post_message(&js_val)
            .map_err(|e| AgentError::Shell(format!("postMessage failed: {:?}", e)))?;

        Ok(Self {
            worker,
            ready: RefCell::new(false),
            next_id: RefCell::new(1),
            pending,
        })
    }

    fn next_exec_id(&self) -> u64 {
        let mut id = self.next_id.borrow_mut();
        let current = *id;
        *id += 1;
        current
    }

    fn send_command(&self, cmd: &WorkerCommand) -> Result<()> {
        let json = serde_json::to_string(cmd)
            .map_err(|e| AgentError::Shell(e.to_string()))?;
        let js_val = js_sys::JSON::parse(&json)
            .map_err(|e| AgentError::Shell(format!("{:?}", e)))?;
        self.worker
            .post_message(&js_val)
            .map_err(|e| AgentError::Shell(format!("{:?}", e)))?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl ShellPort for WasmerShellAdapter {
    async fn execute(&self, cmd: &str, timeout_ms: Option<u64>) -> Result<ExecResult> {
        let id = self.next_exec_id();
        let (sender, receiver) = oneshot::channel();

        self.pending.borrow_mut().insert(
            id,
            PendingExec {
                stdout: String::new(),
                stderr: String::new(),
                sender: Some(sender),
            },
        );

        self.send_command(&WorkerCommand::ExecBash {
            id,
            cmd: cmd.to_string(),
            timeout_ms,
        })?;

        receiver
            .await
            .map_err(|_| AgentError::Shell("Execution channel closed".to_string()))
    }

    fn execute_streaming(
        &self,
        _cmd: &str,
    ) -> Pin<Box<dyn Stream<Item = ShellStreamEvent>>> {
        // Streaming shell will be implemented with mpsc channels in follow-up
        Box::pin(stream::once(async {
            ShellStreamEvent::Error("Streaming not yet implemented".to_string())
        }))
    }

    async fn cancel(&self, handle: ExecHandle) -> Result<()> {
        self.send_command(&WorkerCommand::CancelExec { id: handle.0 })
    }

    fn is_ready(&self) -> bool {
        *self.ready.borrow()
    }
}
