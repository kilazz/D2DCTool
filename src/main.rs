slint::include_modules!();

mod cli;
mod dds;
mod diff;
mod dv2_archive;
mod gui;
mod hash_scanner;
mod logger;
mod pak_tree;
mod tools;
mod xml_converter;

fn main() -> Result<(), slint::PlatformError> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        if let Err(e) = cli::handle_cli(&args) {
            eprintln!("[!] CLI Error: {}", e);
            std::process::exit(1);
        }
        return Ok(());
    }
    gui::run_gui()
}
