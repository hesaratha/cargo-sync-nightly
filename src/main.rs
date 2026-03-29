use std::env;
use std::fs;
use std::path::PathBuf;
use toml_edit::DocumentMut;

fn main() {
    // 1. Fetch the "Fork Day" date from the official beta manifest.
    // This gives us the nightly version that was just promoted to beta.
    let date = match fetch_fork_date() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: could not fetch fork date: {e}");
            std::process::exit(1);
        }
    };

    let channel = format!("nightly-{date}");
    println!("Targeting 'Fork Day' nightly: {channel}");

    // 2. Walk up from cwd to find rust-toolchain.toml.
    let toolchain_path = match find_toolchain_toml() {
        Some(p) => p,
        None => {
            eprintln!("error: could not find rust-toolchain.toml in this directory or any parent");
            std::process::exit(1);
        }
    };
    println!("Found: {}", toolchain_path.display());

    // 3. Update (or create) the channel key.
    match update_toolchain(&toolchain_path, &channel) {
        Ok(_) => println!("Pinned channel = \"{channel}\""),
        Err(e) => {
            eprintln!("error: failed to update toolchain file: {e}");
            std::process::exit(1);
        }
    }
}

/// Fetches the date of the most recent "Fork Day" from the beta manifest.
///
/// In the Rust release model, the 'train' moves every 6 weeks:
/// 1. Friday (Day -6): The master branch is forked/promoted to the Beta channel.
/// 2. Thursday (Day 0): The previous Beta is promoted to Stable.
///
/// By pulling the date from the Beta manifest, we get the exact Friday
/// that the current beta cycle began, allowing us to pin to the most
/// "stable" version of nightly for this release cycle.
fn fetch_fork_date() -> Result<String, Box<dyn std::error::Error>> {
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

/// Walks up the directory tree from cwd, returning the first
/// rust-toolchain.toml found.
fn find_toolchain_toml() -> Option<PathBuf> {
    let mut dir = env::current_dir().ok()?;
    loop {
        let candidate = dir.join("rust-toolchain.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Updates (or sets) `toolchain.channel` in the given rust-toolchain.toml,
/// preserving all other content and formatting.
fn update_toolchain(path: &PathBuf, channel: &str) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let mut doc = content.parse::<DocumentMut>()?;

    // Idiomatic toml_edit 0.25 approach using the Entry API
    doc.entry("toolchain")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))["channel"] =
        toml_edit::value(channel);

    fs::write(path, doc.to_string())?;
    Ok(())
}
