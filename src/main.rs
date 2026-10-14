mod cli;
mod rustup;
mod toolchain;

#[cfg(test)]
mod tests;

use std::env;
use std::ffi::OsStr;
use std::process::{Command, ExitCode};

use cli::Args;
use toolchain::ToolchainFile;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    let args = Args::parse_from(env::args_os()).unwrap_or_else(|error| error.exit());
    report(
        run(&args, fetch_fork_date, OsStr::new("rustup")),
        args.check,
    )
}

fn report(result: Result<ExitCode>, check: bool) -> ExitCode {
    match result {
        Ok(status) => status,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(if check { 2 } else { 1 })
        }
    }
}

fn run(
    args: &Args,
    fetch_date: impl FnOnce() -> Result<String>,
    rustup: &OsStr,
) -> Result<ExitCode> {
    let project = if args.globally {
        println!("Global rustup default");
        None
    } else {
        let path = toolchain::resolve_path(&env::current_dir()?, args.toolchain_file.as_deref())?;
        let file = ToolchainFile::load(&path)?;
        println!("Project toolchain: {}", path.display());
        Some(file)
    };

    let date = fetch_date().map_err(|error| format!("could not fetch fork date: {error}"))?;
    let channel = format!("nightly-{date}");
    println!("Targeting 'Fork Day' nightly: {channel}");

    if let Some(file) = project {
        let current = file.channel().map(str::to_owned);
        let update = file.plan(&channel)?;
        if args.check {
            return Ok(preview(current.as_deref(), &channel, update.is_some()));
        }
        if let Some(update) = update {
            update.apply()?;
            println!("Project channel pinned to \"{channel}\"");
        } else {
            println!("Project channel already pinned to \"{channel}\"");
        }
    } else {
        if args.check {
            let current = rustup::default_toolchain(rustup)?;
            let changed = !rustup::matches_channel(current.as_deref(), &channel);
            return Ok(preview(current.as_deref(), &channel, changed));
        }
        let status = Command::new(rustup)
            .args(["default", &channel])
            .status()
            .map_err(|error| format!("failed to run rustup: {error}"))?;

        if !status.success() {
            return Err(format!("rustup default failed: {status}").into());
        }
        println!("Global default pinned to \"{channel}\"");
    }
    Ok(ExitCode::SUCCESS)
}

fn preview(current: Option<&str>, channel: &str, changed: bool) -> ExitCode {
    if changed {
        println!(
            "Would change channel: {} -> {channel}",
            current.unwrap_or("<unset>")
        );
        ExitCode::FAILURE
    } else {
        println!("Channel already current: {}", current.unwrap_or(channel));
        ExitCode::SUCCESS
    }
}

/// Fetches the date of the most recent "Fork Day" from the beta manifest.
///
/// In the Rust release model, the 'train' moves every 6 weeks:
/// 1. Friday (Day -6): The master branch is forked/promoted to the Beta
///    channel.
/// 2. Thursday (Day 0): The previous Beta is promoted to Stable.
///
/// By pulling the date from the Beta manifest, we get the exact Friday
/// that the current beta cycle began, allowing us to pin to the most
/// "stable" version of nightly for this release cycle.
fn fetch_fork_date() -> Result<String> {
    // ureq 3.x now returns http::Response<ureq::Body>
    let response = ureq::get("https://static.rust-lang.org/dist/channel-rust-beta.toml").call()?;

    // Extract the body and read it to a string
    let body = response.into_body().read_to_string()?;

    // The manifest contains: date = "YYYY-MM-DD"
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("date = ") {
            let date = rest.trim().trim_matches('"').to_string();
            if date.len() == 10 {
                return Ok(date);
            }
        }
    }

    Err("could not find `date` field in beta channel manifest".into())
}
