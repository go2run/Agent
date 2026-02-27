//! Main egui application — composes all panels and manages agent runtime.

use std::rc::Rc;
use std::cell::RefCell;

use egui::{self, CentralPanel, SidePanel, TopBottomPanel, RichText, Vec2};

use agent_core::event_bus::EventBus;
use agent_core::ports::{LlmPort, ShellPort, StoragePort, VfsPort};
use agent_core::runtime::AgentRuntime;
use agent_platform::llm::OpenAiCompatProvider;
use agent_platform::shell::WasmerShellAdapter;
use agent_platform::storage::MemoryStorage;
use agent_platform::vfs::StorageVfs;
use agent_types::config::AgentConfig;
use agent_ui::panels::{chat, terminal, settings};
use agent_ui::state::UiState;
use agent_ui::theme;

const WORKSPACE_ROOT: &str = "/workspace";
const CONFIG_STORAGE_KEY: &str = "agent:config";

/// The main application state
pub struct AgentApp {
    ui_state: UiState,
    config: AgentConfig,
    event_bus: EventBus,
    runtime: Rc<RefCell<AgentRuntime>>,
    llm: Rc<dyn LlmPort>,
    shell: Rc<dyn ShellPort>,
    vfs: Rc<dyn VfsPort>,
    storage: Rc<dyn StoragePort>,
    first_frame: bool,
    font_loaded: Rc<RefCell<bool>>,
}

impl AgentApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = AgentConfig::default();
        let event_bus = EventBus::new();
        let runtime = AgentRuntime::new(config.clone(), event_bus.clone());

        let llm = Rc::new(OpenAiCompatProvider::new(config.llm.clone()));

        let shell: Rc<dyn ShellPort> = match WasmerShellAdapter::new() {
            Ok(s) => Rc::new(s),
            Err(e) => {
                log::warn!("Shell adapter unavailable: {}. Using stub.", e);
                Rc::new(StubShell)
            }
        };

        let storage: Rc<dyn StoragePort> = Rc::new(MemoryStorage::new());
        let vfs: Rc<dyn VfsPort> = Rc::new(StorageVfs::new(storage.clone()));

        let mut app = Self {
            ui_state: UiState::new(),
            config,
            event_bus,
            runtime: Rc::new(RefCell::new(runtime)),
            llm,
            shell,
            vfs: vfs.clone(),
            storage: storage.clone(),
            first_frame: true,
            font_loaded: Rc::new(RefCell::new(false)),
        };

        // Try to restore config from storage
        Self::restore_config(storage.clone(), app.config_ref());

        // Initialize default workspace
        Self::init_workspace(vfs);

        app
    }

    fn config_ref(&mut self) -> Rc<RefCell<Option<AgentConfig>>> {
        // Shared slot for async config restore
        Rc::new(RefCell::new(None))
    }

    /// Restore config from storage (async)
    fn restore_config(storage: Rc<dyn StoragePort>, slot: Rc<RefCell<Option<AgentConfig>>>) {
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(Some(data)) = storage.get(CONFIG_STORAGE_KEY).await {
                if let Ok(config) = serde_json::from_slice::<AgentConfig>(&data) {
                    *slot.borrow_mut() = Some(config);
                    log::info!("Config restored from storage");
                }
            }
        });
    }

    /// Save config to storage (async, fire-and-forget)
    fn save_config(storage: Rc<dyn StoragePort>, config: &AgentConfig) {
        if let Ok(json) = serde_json::to_vec(config) {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = storage.set(CONFIG_STORAGE_KEY, &json).await;
                log::info!("Config saved to storage");
            });
        }
    }

    /// Create default workspace directories in VFS
    fn init_workspace(vfs: Rc<dyn VfsPort>) {
        wasm_bindgen_futures::spawn_local(async move {
            let dirs = [
                WORKSPACE_ROOT,
                &format!("{}/home", WORKSPACE_ROOT),
                &format!("{}/tmp", WORKSPACE_ROOT),
                &format!("{}/src", WORKSPACE_ROOT),
            ];
            for dir in &dirs {
                let _ = vfs.mkdir(dir).await;
            }
            let readme = "# WASM Agent Workspace\n\n\
                This is your default workspace.\n\
                Files created by the agent will be stored here.\n";
            let _ = vfs
                .write_file(
                    &format!("{}/README.md", WORKSPACE_ROOT),
                    readme.as_bytes(),
                )
                .await;
            log::info!("Workspace initialised at {}", WORKSPACE_ROOT);
        });
    }

    /// Fetch CJK font from server and install into egui
    fn load_cjk_font(ctx: egui::Context, loaded_flag: Rc<RefCell<bool>>) {
        wasm_bindgen_futures::spawn_local(async move {
            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            let resp = match wasm_bindgen_futures::JsFuture::from(
                window.fetch_with_str("NotoSansTC-Regular.otf"),
            )
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("Failed to fetch CJK font: {:?}", e);
                    return;
                }
            };
            let resp: web_sys::Response = resp.into();
            let buf = match resp.array_buffer() {
                Ok(p) => match wasm_bindgen_futures::JsFuture::from(p).await {
                    Ok(b) => b,
                    Err(_) => return,
                },
                Err(_) => return,
            };
            let uint8 = js_sys::Uint8Array::new(&buf);
            let bytes = uint8.to_vec();

            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "noto_sans_tc".to_owned(),
                egui::FontData::from_owned(bytes).into(),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "noto_sans_tc".to_owned());
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .push("noto_sans_tc".to_owned());

            ctx.set_fonts(fonts);
            *loaded_flag.borrow_mut() = true;
            ctx.request_repaint();
            log::info!("CJK font loaded");
        });
    }

    fn rebuild_llm(&mut self) {
        self.llm = Rc::new(OpenAiCompatProvider::new(self.config.llm.clone()));
    }
}

impl eframe::App for AgentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.first_frame {
            theme::apply_theme(ctx);
            Self::load_cjk_font(ctx.clone(), self.font_loaded.clone());
            self.first_frame = false;
        }

        // Drain events from the agent runtime
        let events = self.event_bus.drain();
        if !events.is_empty() {
            self.ui_state.process_events(events);
            ctx.request_repaint();
        }

        if self.ui_state.is_busy() {
            ctx.request_repaint();
        }

        // ── Top bar ──────────────────────────────────────────
        TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("WASM Agent")
                        .strong()
                        .color(theme::ACCENT)
                        .size(16.0),
                );
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "Provider: {} | Model: {}",
                        self.config.llm.provider.label(),
                        self.config.llm.model
                    ))
                    .color(theme::TEXT_SECONDARY)
                    .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_label(self.ui_state.show_settings, "Settings")
                        .clicked()
                    {
                        self.ui_state.show_settings = !self.ui_state.show_settings;
                    }
                });
            });
        });

        // ── Settings side panel ──────────────────────────────
        if self.ui_state.show_settings {
            SidePanel::right("settings_panel")
                .min_width(280.0)
                .max_width(350.0)
                .show(ctx, |ui| {
                    if settings::settings_panel(ui, &mut self.config) {
                        self.rebuild_llm();
                        Self::save_config(self.storage.clone(), &self.config);
                    }
                });
        }

        // ── Main content ─────────────────────────────────────
        CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let terminal_height = (available.y * 0.3).max(120.0);

            // Chat panel (top)
            let chat_height = available.y - terminal_height - 12.0;
            ui.allocate_ui(Vec2::new(available.x, chat_height), |ui| {
                if let Some(user_msg) = chat::chat_panel(ui, &mut self.ui_state) {
                    self.dispatch_message(user_msg, ctx);
                }
            });

            ui.add_space(4.0);

            // Terminal panel (bottom)
            ui.allocate_ui(Vec2::new(available.x, terminal_height), |ui| {
                if let Some(cmd) = terminal::terminal_panel(ui, &mut self.ui_state) {
                    self.dispatch_shell_command(cmd, ctx);
                }
            });
        });
    }
}

impl AgentApp {
    /// Dispatch a user message to the agent runtime (async)
    fn dispatch_message(&self, text: String, ctx: &egui::Context) {
        let runtime = self.runtime.clone();
        let llm = self.llm.clone();
        let shell = self.shell.clone();
        let vfs = self.vfs.clone();
        let ctx = ctx.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let result = {
                let mut rt = runtime.borrow_mut();
                rt.run_turn(&text, llm.as_ref(), shell.as_ref(), vfs.as_ref())
                    .await
            };
            if let Err(e) = result {
                log::error!("Agent turn error: {}", e);
            }
            ctx.request_repaint();
        });
    }

    /// Execute a shell command directly from the terminal (async)
    fn dispatch_shell_command(&self, cmd: String, ctx: &egui::Context) {
        let shell = self.shell.clone();
        let event_bus = self.event_bus.clone();
        let ctx = ctx.clone();

        wasm_bindgen_futures::spawn_local(async move {
            match shell.execute(&cmd, None).await {
                Ok(result) => {
                    if !result.stdout.is_empty() {
                        for line in result.stdout.lines() {
                            event_bus.emit(agent_types::event::AgentEvent::ToolOutput {
                                call_id: String::new(),
                                chunk: line.to_string(),
                            });
                        }
                    }
                    if !result.stderr.is_empty() {
                        for line in result.stderr.lines() {
                            event_bus.emit(agent_types::event::AgentEvent::ToolOutput {
                                call_id: String::new(),
                                chunk: format!("stderr: {}", line),
                            });
                        }
                    }
                }
                Err(e) => {
                    event_bus.emit(agent_types::event::AgentEvent::Error {
                        message: format!("Shell error: {}", e),
                    });
                }
            }
            ctx.request_repaint();
        });
    }
}

// ─── Stub shell for when Worker is not available ─────────────

struct StubShell;

#[async_trait::async_trait(?Send)]
impl ShellPort for StubShell {
    async fn execute(
        &self,
        cmd: &str,
        _timeout_ms: Option<u64>,
    ) -> agent_types::Result<agent_types::tool::ExecResult> {
        Ok(agent_types::tool::ExecResult {
            stdout: format!(
                "[Shell not available] Would execute: {}\n\
                 Hint: Wasmer-JS Worker failed to initialize. \
                 Ensure worker.js is served correctly.",
                cmd
            ),
            stderr: String::new(),
            exit_code: 127,
        })
    }

    fn execute_streaming(
        &self,
        _cmd: &str,
    ) -> std::pin::Pin<Box<dyn futures::Stream<Item = agent_core::ports::ShellStreamEvent>>> {
        Box::pin(futures::stream::once(async {
            agent_core::ports::ShellStreamEvent::Error("Shell not available".to_string())
        }))
    }

    async fn cancel(&self, _handle: agent_types::tool::ExecHandle) -> agent_types::Result<()> {
        Ok(())
    }

    fn is_ready(&self) -> bool {
        false
    }
}
