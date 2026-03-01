//! Settings panel — LLM provider config, storage mode, API key input.
//! Now includes explicit Save button with visual feedback.

use egui::{self, RichText, Vec2};
use agent_types::config::{AgentConfig, LlmProvider, StorageBackendType};
use crate::theme::*;

/// What the caller should do after rendering the settings panel
pub enum SettingsAction {
    /// Nothing changed
    None,
    /// A field was changed (auto-save)
    Changed,
    /// The user clicked the explicit Save button
    SaveClicked,
}

/// Save feedback passed in from the app layer
#[derive(Clone)]
pub struct SaveFeedback {
    pub message: String,
    pub success: bool,
}

/// Render the settings panel. Returns an action for the caller to handle.
pub fn settings_panel(
    ui: &mut egui::Ui,
    config: &mut AgentConfig,
    save_feedback: Option<&SaveFeedback>,
) -> SettingsAction {
    let mut changed = false;
    let mut save_clicked = false;

    egui::Frame::default()
        .fill(BG_SECONDARY)
        .inner_margin(PANEL_PADDING)
        .corner_radius(PANEL_ROUNDING)
        .show(ui, |ui| {
            ui.heading(RichText::new("Settings").color(TEXT_PRIMARY));
            ui.separator();

            // ── LLM Section ──────────────────────────────────
            ui.label(RichText::new("LLM").color(ACCENT).strong());
            ui.add_space(2.0);

            // Provider
            ui.label(RichText::new("Provider").color(TEXT_SECONDARY).small());
            egui::ComboBox::from_id_salt("llm_provider")
                .selected_text(config.llm.provider.label())
                .show_ui(ui, |ui| {
                    for p in LlmProvider::all() {
                        if ui
                            .selectable_value(&mut config.llm.provider, p.clone(), p.label())
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });

            ui.add_space(4.0);

            // Model
            ui.label(RichText::new("Model").color(TEXT_SECONDARY).small());
            if ui
                .text_edit_singleline(&mut config.llm.model)
                .changed()
            {
                changed = true;
            }

            ui.add_space(4.0);

            // API Key (masked)
            ui.label(RichText::new("API Key").color(TEXT_SECONDARY).small());
            let api_key_edit = egui::TextEdit::singleline(&mut config.llm.api_key)
                .password(true)
                .hint_text("sk-...");
            if ui.add(api_key_edit).changed() {
                changed = true;
            }

            ui.add_space(4.0);

            // Custom base URL
            ui.label(RichText::new("API Base URL (optional)").color(TEXT_SECONDARY).small());
            let mut base_url = config.llm.api_base.clone().unwrap_or_default();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut base_url)
                        .hint_text(config.llm.provider.default_base_url()),
                )
                .changed()
            {
                config.llm.api_base = if base_url.is_empty() {
                    None
                } else {
                    Some(base_url)
                };
                changed = true;
            }

            ui.add_space(4.0);

            // Temperature
            ui.label(RichText::new("Temperature").color(TEXT_SECONDARY).small());
            if ui
                .add(egui::Slider::new(&mut config.llm.temperature, 0.0..=2.0))
                .changed()
            {
                changed = true;
            }

            // Max tokens
            ui.label(RichText::new("Max Tokens").color(TEXT_SECONDARY).small());
            if ui
                .add(egui::Slider::new(&mut config.llm.max_tokens, 256..=32768))
                .changed()
            {
                changed = true;
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(4.0);

            // ── Storage Section ──────────────────────────────
            ui.label(RichText::new("Storage").color(ACCENT).strong());
            ui.add_space(2.0);

            ui.label(RichText::new("Backend").color(TEXT_SECONDARY).small());
            egui::ComboBox::from_id_salt("storage_backend")
                .selected_text(storage_label(&config.storage.backend))
                .show_ui(ui, |ui| {
                    for (backend, label, _desc) in storage_options() {
                        if ui
                            .selectable_value(&mut config.storage.backend, backend, label)
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });

            ui.add_space(4.0);
            ui.label(
                RichText::new(storage_description(&config.storage.backend))
                    .color(TEXT_SECONDARY)
                    .small()
                    .italics(),
            );

            // ── Save Button ──────────────────────────────────
            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let btn = ui.add(
                    egui::Button::new(
                        RichText::new("Save Settings")
                            .color(TEXT_PRIMARY)
                            .strong(),
                    )
                    .fill(ACCENT)
                    .corner_radius(PANEL_ROUNDING)
                    .min_size(Vec2::new(120.0, 28.0)),
                );
                if btn.clicked() {
                    save_clicked = true;
                }

                // Show save feedback
                if let Some(fb) = save_feedback {
                    let color = if fb.success { SUCCESS } else { ERROR };
                    ui.label(
                        RichText::new(&fb.message)
                            .color(color)
                            .small(),
                    );
                }
            });
        });

    if save_clicked {
        SettingsAction::SaveClicked
    } else if changed {
        SettingsAction::Changed
    } else {
        SettingsAction::None
    }
}

fn storage_label(backend: &StorageBackendType) -> &'static str {
    match backend {
        StorageBackendType::Auto => "Auto-detect",
        StorageBackendType::Memory => "Memory",
        StorageBackendType::IndexedDb => "IndexedDB",
        StorageBackendType::Opfs => "OPFS",
    }
}

fn storage_description(backend: &StorageBackendType) -> &'static str {
    match backend {
        StorageBackendType::Auto => "Automatically selects the best available backend. Tries IndexedDB first, falls back to Memory.",
        StorageBackendType::Memory => "Fast but volatile. All data is lost on page reload.",
        StorageBackendType::IndexedDb => "Persistent browser storage. Data survives page reloads and browser restarts.",
        StorageBackendType::Opfs => "Origin Private File System. High-performance persistent storage (experimental).",
    }
}

fn storage_options() -> Vec<(StorageBackendType, &'static str, &'static str)> {
    vec![
        (StorageBackendType::Auto, "Auto-detect", "Best available"),
        (StorageBackendType::Memory, "Memory", "Fast, volatile"),
        (StorageBackendType::IndexedDb, "IndexedDB", "Persistent"),
        (StorageBackendType::Opfs, "OPFS", "Experimental"),
    ]
}
