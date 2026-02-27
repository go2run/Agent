//! Terminal panel — displays bash output from tool executions.

use egui::{self, RichText, ScrollArea};
use crate::state::UiState;
use crate::theme::*;

/// Render the terminal output panel.
pub fn terminal_panel(ui: &mut egui::Ui, state: &UiState) {
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
            });

            ui.separator();

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if state.terminal_lines.is_empty() {
                        ui.label(
                            RichText::new("No output yet. The agent will display bash output here.")
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
        });
}
