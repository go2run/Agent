//! Agent App — WASM entry point.
//!
//! This crate is the composition root (DI wiring layer).
//! It assembles all platform adapters and hands them to the egui UI.

mod app;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// WASM entry point — called from index.html
#[wasm_bindgen(start)]
pub async fn main() {
    // Initialize logging
    wasm_logger::init(wasm_logger::Config::default());
    log::info!("Agent WASM starting...");

    // Launch the egui application
    let web_options = eframe::WebOptions::default();

    // Get the canvas element by ID
    let document = web_sys::window()
        .expect("No window")
        .document()
        .expect("No document");
    let canvas = document
        .get_element_by_id("agent_canvas")
        .expect("No canvas element with id 'agent_canvas'")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("Element is not a canvas");

    wasm_bindgen_futures::spawn_local(async move {
        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|cc| Ok(Box::new(app::AgentApp::new(cc)))),
            )
            .await
            .expect("Failed to start eframe");
    });
}
