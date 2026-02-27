//! Chat panel — displays conversation messages and input field.

use egui::{self, Align, Color32, Layout, RichText, ScrollArea, Vec2};
use crate::state::UiState;
use crate::theme::*;

/// Render the chat panel. Returns Some(message) when user submits input.
pub fn chat_panel(ui: &mut egui::Ui, state: &mut UiState) -> Option<String> {
    let mut submitted = None;

    egui::Frame::default()
        .fill(BG_PRIMARY)
        .inner_margin(PANEL_PADDING)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                // Header
                ui.horizontal(|ui| {
                    ui.heading(
                        RichText::new("Agent Chat")
                            .color(TEXT_PRIMARY)
                            .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let status_color = if state.is_busy() { WARNING } else { SUCCESS };
                        ui.label(
                            RichText::new(&state.status_text)
                                .color(status_color)
                                .small(),
                        );
                    });
                });

                ui.separator();

                // Messages area
                let available_height = ui.available_height() - 60.0;
                ScrollArea::vertical()
                    .max_height(available_height)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for entry in &state.messages {
                            render_message(ui, entry);
                            ui.add_space(4.0);
                        }

                        // Show streaming text if any
                        if !state.streaming_text.is_empty() {
                            egui::Frame::default()
                                .fill(BG_SECONDARY)
                                .corner_radius(PANEL_ROUNDING)
                                .inner_margin(8.0)
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(&state.streaming_text)
                                            .color(TEXT_PRIMARY),
                                    );
                                    ui.label(
                                        RichText::new("▌")
                                            .color(ACCENT)
                                            .strong(),
                                    );
                                });
                        }
                    });

                ui.add_space(8.0);

                // Input area
                ui.horizontal(|ui| {
                    let input = egui::TextEdit::singleline(&mut state.input_text)
                        .hint_text("Type a message...")
                        .desired_width(ui.available_width() - 70.0)
                        .font(egui::FontId::proportional(14.0));

                    let response = ui.add(input);

                    let send_enabled = !state.input_text.trim().is_empty() && !state.is_busy();
                    let send_btn = ui.add_enabled(
                        send_enabled,
                        egui::Button::new(
                            RichText::new("Send").color(TEXT_PRIMARY),
                        )
                        .fill(if send_enabled { ACCENT } else { BG_SURFACE })
                        .corner_radius(PANEL_ROUNDING)
                        .min_size(Vec2::new(60.0, 0.0)),
                    );

                    // Submit on Enter or button click
                    if (response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !state.input_text.trim().is_empty()
                        && !state.is_busy())
                        || send_btn.clicked()
                    {
                        let text = state.input_text.trim().to_string();
                        state.push_user_message(&text);
                        submitted = Some(text);
                        state.input_text.clear();
                        response.request_focus();
                    }
                });
            });
        });

    submitted
}

fn render_message(ui: &mut egui::Ui, entry: &crate::state::ChatEntry) {
    let error_bg = Color32::from_rgb(50, 20, 20);
    let (label, label_color, bg) = match entry.role.as_str() {
        "user" => ("You", ACCENT, BG_SECONDARY),
        "assistant" => ("Agent", SUCCESS, BG_SECONDARY),
        "tool" => ("[tool]", WARNING, BG_SURFACE),
        "error" => ("Error", ERROR, error_bg),
        _ => ("???", TEXT_SECONDARY, BG_SECONDARY),
    };

    egui::Frame::default()
        .fill(bg)
        .corner_radius(PANEL_ROUNDING)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.label(RichText::new(label).color(label_color).strong().small());
            ui.label(RichText::new(&entry.content).color(TEXT_PRIMARY));
        });
}
