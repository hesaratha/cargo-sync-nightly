use std::process::Command;

fn main() {
    let date = match fetch_fork_date() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: could not fetch fork date: {e}");
            std::process::exit(1);
        }
    };
    let channel = format!("nightly-{date}");
    println!("Targeting 'Fork Day' nightly: {channel}");

    // Set it as the global rustup default
    let status = Command::new("rustup")
        .args(["default", &channel])
        .status()
        .expect("failed to run rustup");

    if !status.success() {
        eprintln!("error: rustup default failed");
        std::process::exit(1);
    }
    println!("Global default pinned to \"{channel}\"");
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
