// fingerprint_explorer_cmd.rs — Launch the interactive fingerprint explorer TUI
use std::process;

pub fn execute(strip_image: Option<String>) {
    let strip_path = match strip_image {
        Some(ref p) => std::path::PathBuf::from(p),
        None => {
            eprintln!("Error: FINGERPRINT_EXPLORE_MISSING_ARG -- strip image path is required");
            process::exit(1);
        }
    };

    if !strip_path.exists() {
        eprintln!(
            "Error: FINGERPRINT_EXPLORE_FILE_NOT_FOUND -- file not found: {}",
            strip_path.display()
        );
        process::exit(1);
    }

    if let Err(e) = crate::tui::fingerprint_explorer::run(&strip_path) {
        eprintln!("Error: FINGERPRINT_EXPLORE_ERROR -- {}", e);
        process::exit(1);
    }
}
