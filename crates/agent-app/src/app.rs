//! Main egui application — composes all panels and manages agent runtime.

use std::rc::Rc;
use std::cell::RefCell;

use egui::{self, CentralPanel, SidePanel, TopBottomPanel, RichText, Vec2};

use agent_core::event_bus::EventBus;
use agent_core::ports::{LlmPort, ShellPort, VfsPort};
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

/// The main application state
pub struct AgentApp {
    ui_state: UiState,
    config: AgentConfig,
    event_bus: EventBus,
    /// Agent runtime wrapped in RefCell for interior mutability in async tasks
    runtime: Rc<RefCell<AgentRuntime>>,
    /// LLM provider — recreated when config changes
    llm: Rc<dyn LlmPort>,
    /// Shell adapter
    shell: Rc<dyn ShellPort>,
    /// Virtual filesystem
    vfs: Rc<dyn VfsPort>,
    /// First frame flag for theme + font setup
    first_frame: bool,
    /// Whether CJK font has been loaded
    font_loaded: Rc<RefCell<bool>>,
}

impl AgentApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = AgentConfig::default();
        let event_bus = EventBus::new();

        // Create the agent runtime
        let runtime = AgentRuntime::new(config.clone(), event_bus.clone());

        // Create platform adapters
        let llm = Rc::new(OpenAiCompatProvider::new(config.llm.clone()));

        // Try to create shell adapter, fall back to a stub if Worker creation fails
        let shell: Rc<dyn ShellPort> = match WasmerShellAdapter::new() {
            Ok(s) => Rc::new(s),
            Err(e) => {
                log::warn!("Shell adapter unavailable: {}. Using stub.", e);
                Rc::new(StubShell)
            }
        };

        // Use memory storage + VFS for now (IndexedDB will be initialized async)
        let storage = Rc::new(MemoryStorage::new());
        let vfs = Rc::new(StorageVfs::new(storage));

        let app = Self {
            ui_state: UiState::new(),
            config,
            event_bus,
            runtime: Rc::new(RefCell::new(runtime)),
            llm,
            shell,
            vfs: vfs.clone(),
            first_frame: true,
            font_loaded: Rc::new(RefCell::new(false)),
        };

        // Initialize default workspace
        Self::init_workspace(vfs);

        app
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
            // Write a welcome README
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
            // Prepend CJK font so it takes priority for CJK glyphs
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
        // Apply theme + start font loading on first frame
        if self.first_frame {
            theme::apply_theme(ctx);
            Self::load_cjk_font(ctx.clone(), self.font_loaded.clone());
            self.first_frame = false;
        }

        // Drain events from the agent runtime and update UI state
        let events = self.event_bus.drain();
        if !events.is_empty() {
            self.ui_state.process_events(events);
            ctx.request_repaint();
        }

        // Request repaint while agent is busy (to poll for events)
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

        // ── Settings side panel (conditionally shown) ────────
        if self.ui_state.show_settings {
            SidePanel::right("settings_panel")
                .min_width(280.0)
                .max_width(350.0)
                .show(ctx, |ui| {
                    if settings::settings_panel(ui, &mut self.config) {
                        self.rebuild_llm();
                    }
                });
        }

        // ── Main content ─────────────────────────────────────
        CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let terminal_height = (available.y * 0.3).max(100.0);

            // Chat panel (top portion)
            let chat_height = available.y - terminal_height - 12.0;
            ui.allocate_ui(Vec2::new(available.x, chat_height), |ui| {
                if let Some(user_msg) = chat::chat_panel(ui, &mut self.ui_state) {
                    self.dispatch_message(user_msg, ctx);
                }
            });

            ui.add_space(4.0);

            // Terminal panel (bottom portion)
            ui.allocate_ui(Vec2::new(available.x, terminal_height), |ui| {
                terminal::terminal_panel(ui, &self.ui_state);
            });
        });
    }
}

impl AgentApp {
    /// Dispatch a user message to the agent runtime (async, non-blocking).
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
