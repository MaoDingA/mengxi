use std::process;


pub fn execute(show: bool, edit: bool) {
    match (show, edit) {
        (true, false) => {
            match crate::config::load_or_create_config() {
                Ok(cfg) => println!("{cfg}"),
                Err(e) => {
                    eprintln!("Error: CONFIG_LOAD_FAILED — {e}");
                    process::exit(1);
                }
            }
        }
        (false, true) => {
            eprintln!("Error: 'config --edit' is not yet implemented");
            process::exit(1);
        }
        (false, false) | (true, true) => {
            eprintln!("Error: Specify --show or --edit");
            process::exit(1);
        }
    }
}
