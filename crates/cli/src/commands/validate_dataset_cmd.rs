use std::process;

use crate::validate_dataset;

pub fn execute(dir: String, format: String) {
    let is_json = format == "json";
    let exit_code = validate_dataset::run_validate_dataset(&dir, is_json);
    process::exit(exit_code);
}
