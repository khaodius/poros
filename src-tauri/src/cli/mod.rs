//! The headless command line, `poros-cli`: what the app does with servers, for scripts and
//! agents. Each run connects once, does one thing (or a batch of them), and disconnects. It
//! never prompts. `--json` prints one JSON document on standard output, errors included, and
//! the exit status says how it went (see `exit::TABLE`).
//!
//! It reads the app's settings, saved connections and trusted host keys from the app's config
//! folder but writes nothing there, except a host key it is told to remember and a cloud
//! sign-in the provider renewed. Its transfer queue is its own and is never saved.

mod args;
mod batch;
mod commands;
mod context;
mod exit;
mod logging;
mod output;
mod progress;
mod target;
mod transfers;

use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use tokio_util::sync::CancellationToken;

use args::{Cli, Command};
use context::Context;
use exit::Exit;
use output::{error_value, print_error, print_json, print_text};

use crate::error::AppError;

/// After Ctrl+C, a command gets this long to stop cleanly before the run ends regardless.
const INTERRUPT_GRACE: Duration = Duration::from_secs(3);

pub fn main() -> ExitCode {
    let json_requested = std::env::args_os().any(|argument| argument == "--json")
        || std::env::var("POROS_JSON").is_ok_and(|value| is_truthy(&value));
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) if !error.use_stderr() => {
            // --help and --version.
            let _ = error.print();
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            if json_requested {
                let message = error.render().to_string();
                let message = message.trim().trim_start_matches("error: ");
                print_json(&error_value(&AppError::invalid(message)));
            } else {
                let _ = error.print();
            }
            return Exit::Usage.into();
        }
    };
    logging::install(cli.global.verbose, cli.global.quiet);
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("poros-cli: could not start: {error}");
            return Exit::Failure.into();
        }
    };
    // With the app built in, the core spawns on Tauri's runtime; this makes that the same one.
    #[cfg(feature = "gui")]
    tauri::async_runtime::set(runtime.handle().clone());
    let exit = runtime.block_on(run(cli));
    runtime.shutdown_timeout(Duration::from_secs(1));
    exit.into()
}

fn is_truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "f" | "false" | "n" | "no" | "off"
    )
}

async fn run(cli: Cli) -> Exit {
    let json = cli.global.json;
    let interrupt = CancellationToken::new();
    {
        let interrupt = interrupt.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                interrupt.cancel();
                // A second Ctrl+C ends the run at once.
                if tokio::signal::ctrl_c().await.is_ok() {
                    std::process::exit(Exit::Interrupted.code().into());
                }
            }
        });
    }
    let needs_connection = cli.command.needs_connection();
    let context = match Context::new(cli.global, interrupt.clone()) {
        Ok(context) => context,
        Err(error) => return report(&error, json),
    };
    let work = async {
        match &cli.command {
            Command::Batch => batch::run(&context).await,
            command => commands::run(&context, command, json, None).await,
        }
    };
    let outcome = tokio::select! {
        outcome = work => outcome,
        _ = async {
            interrupt.cancelled().await;
            tokio::time::sleep(INTERRUPT_GRACE).await;
        } => Err(AppError::cancelled()),
    };
    if needs_connection {
        logging::silence();
        context.close().await;
    }
    match outcome {
        Ok(reply) => {
            if json {
                if !reply.value.is_null() {
                    print_json(&reply.value);
                }
            } else {
                print_text(&reply.text);
            }
            reply.exit
        }
        Err(error) => report(&error, json),
    }
}

fn report(error: &AppError, json: bool) -> Exit {
    if json {
        print_json(&error_value(error));
    } else {
        print_error(error);
    }
    Exit::for_error(error.kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_from_the_environment_reads_like_a_flag() {
        assert!(is_truthy("1") && is_truthy("true") && is_truthy("yes"));
        assert!(!is_truthy("0") && !is_truthy("false") && !is_truthy(" "));
    }
}
