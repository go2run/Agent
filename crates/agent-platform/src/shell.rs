//! Shell adapter — bridges to @wasmer/sdk running in a Web Worker.
//!
//! Architecture:
//! - Main thread (egui) ←→ Web Worker (@wasmer/sdk + WASIX bash)
//! - Communication via postMessage with JSON-serialized WorkerCommand/WorkerEvent
//! - The Worker loads @wasmer/sdk and spawns WASIX bash processes
//!
//! The worker is created as a module worker (`type: "module"`) so it can
//! use ES module `import` to load @wasmer/sdk.

use std::cell::RefCell;
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::Rc;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::stream::{self, Stream};
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, Worker, WorkerOptions, WorkerType};

use agent_core::ports::{ShellPort, ShellStreamEvent};
use agent_types::{
    AgentError, Result,
    event::{WorkerCommand, WorkerEvent},
    tool::{ExecHandle, ExecResult},
};

/// Shell adapter that communicates with @wasmer/sdk via a module Web Worker.
pub struct WasmerShellAdapter {
    worker: Worker,
    ready: Rc<RefCell<bool>>,
    next_id: RefCell<u64>,
    /// Pending one-shot results, keyed by execution ID
    pending: Rc<RefCell<HashMap<u64, PendingExec>>>,
    /// Streaming output channels, keyed by execution ID
    streaming: Rc<RefCell<HashMap<u64, mpsc::UnboundedSender<ShellStreamEvent>>>>,
}

struct PendingExec {
    stdout: String,
    stderr: String,
    sender: Option<oneshot::Sender<ExecResult>>,
}

impl WasmerShellAdapter {
    /// Create a new shell adapter. Spawns a module Web Worker.
    pub fn new() -> Result<Self> {
        // Create a module worker so it can use ES module imports
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);

        let worker = Worker::new_with_options("./worker.js", &options)
            .map_err(|e| AgentError::Shell(format!("Failed to create module worker: {:?}", e)))?;

        let pending: Rc<RefCell<HashMap<u64, PendingExec>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let streaming: Rc<RefCell<HashMap<u64, mpsc::UnboundedSender<ShellStreamEvent>>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let ready = Rc::new(RefCell::new(false));

        // Set up message handler for worker events
        let pending_clone = pending.clone();
        let streaming_clone = streaming.clone();
        let ready_clone = ready.clone();
        let onmessage = Closure::wrap(Box::new(move |event: MessageEvent| {
            let data = event.data();
            if let Ok(json_str) = js_sys::JSON::stringify(&data) {
                let s: String = json_str.into();
                if let Ok(worker_event) = serde_json::from_str::<WorkerEvent>(&s) {
                    match worker_event {
                        WorkerEvent::Ready => {
                            *ready_clone.borrow_mut() = true;
                            log::info!("Wasmer-JS worker ready (@wasmer/sdk)");
                        }
                        WorkerEvent::Stdout { id, data } => {
                            // Forward to streaming channel if one exists
                            if let Some(tx) = streaming_clone.borrow().get(&id) {
                                let _ = tx.unbounded_send(ShellStreamEvent::Stdout(data.clone()));
                            }
                            // Also accumulate for non-streaming callers
                            if let Some(exec) = pending_clone.borrow_mut().get_mut(&id) {
                                exec.stdout.push_str(&data);
                            }
                        }
                        WorkerEvent::Stderr { id, data } => {
                            if let Some(tx) = streaming_clone.borrow().get(&id) {
                                let _ = tx.unbounded_send(ShellStreamEvent::Stderr(data.clone()));
                            }
                            if let Some(exec) = pending_clone.borrow_mut().get_mut(&id) {
                                exec.stderr.push_str(&data);
                            }
                        }
                        WorkerEvent::ExitCode { id, code } => {
                            // Close streaming channel
                            if let Some(tx) = streaming_clone.borrow_mut().remove(&id) {
                                let _ = tx.unbounded_send(ShellStreamEvent::Exit(code));
                            }
                            // Resolve one-shot
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
                            // Close streaming channel with error
                            if let Some(tx) = streaming_clone.borrow_mut().remove(&id) {
                                let _ = tx.unbounded_send(ShellStreamEvent::Error(message.clone()));
                            }
                            // Resolve one-shot with error
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
            ready: Rc::new(RefCell::new(false)),
            next_id: RefCell::new(1),
            pending,
            streaming,
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
        cmd: &str,
    ) -> Pin<Box<dyn Stream<Item = ShellStreamEvent>>> {
        let id = self.next_exec_id();
        let (tx, rx) = mpsc::unbounded();

        // Register the streaming channel
        self.streaming.borrow_mut().insert(id, tx);

        // Also register a pending exec (for cleanup)
        self.pending.borrow_mut().insert(
            id,
            PendingExec {
                stdout: String::new(),
                stderr: String::new(),
                sender: None,
            },
        );

        // Send the exec command
        if let Err(e) = self.send_command(&WorkerCommand::ExecBash {
            id,
            cmd: cmd.to_string(),
            timeout_ms: None,
        }) {
            return Box::pin(stream::once(async move {
                ShellStreamEvent::Error(format!("Failed to send command: {}", e))
            }));
        }

        Box::pin(rx)
    }

    async fn cancel(&self, handle: ExecHandle) -> Result<()> {
        self.send_command(&WorkerCommand::CancelExec { id: handle.0 })
    }

    fn is_ready(&self) -> bool {
        *self.ready.borrow()
    }
}
