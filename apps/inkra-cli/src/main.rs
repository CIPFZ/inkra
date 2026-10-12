//! `inkra-cli`: command line interface (CM-002).

use std::process::ExitCode;

const USAGE: &str = "\
Usage: inkra-cli <COMMAND> [ARGS]

Commands:
  commands     List all commands
  version      Print version

Options:
  -V, --version  Print version
  -h, --help     Print help
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-V" | "--version" | "version") => {
            println!("inkra-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("-h" | "--help") | None => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("commands") => {
            for c in inkra_engine::builtin_registry().iter() {
                println!("{}", c.spec().id);
            }
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("inkra-cli: unknown command `{other}`\n\n{USAGE}");
            ExitCode::from(1)
        }
    }
}
