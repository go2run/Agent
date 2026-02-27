//! Terminal panel — interactive terminal with input and output.

use egui::{self, RichText, ScrollArea, Vec2};
use crate::state::UiState;
use crate::theme::*;

/// Render the terminal panel. Returns Some(command) when user submits a command.
pub fn terminal_panel(ui: &mut egui::Ui, state: &mut UiState) -> Option<String> {
    let mut submitted = None;

    egui::Frame::default()
        .fill(TERMINAL_BG)
        .inner_margin(PANEL_PADDING)
        .corner_radius(PANEL_ROUNDING)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Terminal")
                        .color(TERMINAL_FG)
                        .strong()
                        .monospace(),
                );
                ui.label(
                    RichText::new(format!(" ({} lines)", state.terminal_lines.len()))
                        .color(TEXT_SECONDARY)
                        .small()
                        .monospace(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(RichText::new("Clear").color(TEXT_SECONDARY).monospace())
                        .clicked()
                    {
                        state.terminal_lines.clear();
                    }
                });
            });

            ui.separator();

            // Output area
            let input_height = 28.0;
            let output_height = ui.available_height() - input_height - 8.0;

            ScrollArea::vertical()
                .max_height(output_height)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if state.terminal_lines.is_empty() {
                        ui.label(
                            RichText::new("$ _")
                                .color(TEXT_SECONDARY)
                                .italics()
                                .monospace(),
                        );
                    } else {
                        for line in &state.terminal_lines {
                            let color = if line.is_stderr {
                                TERMINAL_ERR
                            } else {
                                TERMINAL_FG
                            };
                            ui.label(
                                RichText::new(&line.text)
                                    .color(color)
                                    .monospace(),
                            );
                        }
                    }
                });

            // Input area
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("$")
                        .color(ACCENT)
                        .strong()
                        .monospace(),
                );
                let input = egui::TextEdit::singleline(&mut state.terminal_input)
                    .font(egui::FontId::monospace(13.0))
                    .text_color(TERMINAL_FG)
                    .hint_text("Enter command...")
                    .desired_width(ui.available_width() - 60.0);

                let response = ui.add(input);

                let can_send = !state.terminal_input.trim().is_empty();
                let run_btn = ui.add_enabled(
                    can_send,
                    egui::Button::new(
                        RichText::new("Run").color(TEXT_PRIMARY).monospace(),
                    )
                    .fill(if can_send { ACCENT } else { BG_SURFACE })
                    .corner_radius(PANEL_ROUNDING)
                    .min_size(Vec2::new(48.0, 0.0)),
                );

                if (response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && can_send)
                    || run_btn.clicked()
                {
                    let cmd = state.terminal_input.trim().to_string();
                    // Echo command to terminal
                    state.terminal_lines.push(crate::state::TerminalLine {
                        text: format!("$ {}", cmd),
                        is_stderr: false,
                    });
                    // Store in history
                    state.command_history.push(cmd.clone());
                    state.history_index = None;
                    submitted = Some(cmd);
                    state.terminal_input.clear();
                    response.request_focus();
                }

                // Up/Down arrow for history navigation
                if response.has_focus() {
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        let hist_len = state.command_history.len();
                        if hist_len > 0 {
                            let idx = match state.history_index {
                                Some(i) if i > 0 => i - 1,
                                Some(i) => i,
                                None => hist_len - 1,
                            };
                            state.history_index = Some(idx);
                            state.terminal_input = state.command_history[idx].clone();
                        }
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        let hist_len = state.command_history.len();
                        if let Some(idx) = state.history_index {
                            if idx + 1 < hist_len {
                                state.history_index = Some(idx + 1);
                                state.terminal_input = state.command_history[idx + 1].clone();
                            } else {
                                state.history_index = None;
                                state.terminal_input.clear();
                            }
                        }
                    }
                }
            });
        });

    submitted
}
