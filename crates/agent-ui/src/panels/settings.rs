//! Settings panel — LLM provider config, model selection, API key input.

use egui::{self, RichText};
use agent_types::config::{AgentConfig, LlmProvider};
use crate::theme::*;

/// Render the settings panel. Returns true if settings were modified.
pub fn settings_panel(ui: &mut egui::Ui, config: &mut AgentConfig) -> bool {
    let mut changed = false;

    egui::Frame::default()
        .fill(BG_SECONDARY)
        .inner_margin(PANEL_PADDING)
        .corner_radius(PANEL_ROUNDING)
        .show(ui, |ui| {
            ui.heading(RichText::new("Settings").color(TEXT_PRIMARY));
            ui.separator();

            // LLM Provider
            ui.label(RichText::new("LLM Provider").color(TEXT_SECONDARY).small());
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
        });

    changed
}
