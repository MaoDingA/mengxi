use std::process;

use crate::validate;

pub fn execute(format: String, full: bool) {
    let is_json = format == "json";
    let exit_code = validate::run_validate(is_json, full);
    process::exit(exit_code);
}
