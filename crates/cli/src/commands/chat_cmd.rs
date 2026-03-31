use std::process;

use crate::tui;

pub fn execute(provider: String, model: Option<String>) {
    let model_str = model.as_deref().unwrap_or("");
    if let Err(e) = tui::run(&provider, model_str) {
        eprintln!("TUI_ERROR -- {}", e);
        process::exit(1);
    }
}
