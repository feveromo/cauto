use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = cauto::cli::Cli::parse();
    match cauto::run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("cauto: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}
