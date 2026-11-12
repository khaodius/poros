//! The headless command line. Everything lives in `poros_lib::cli`, next to the core it drives.

fn main() -> std::process::ExitCode {
    poros_lib::cli::main()
}
