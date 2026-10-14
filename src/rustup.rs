use std::ffi::OsStr;
use std::process::Command;

use crate::Result;

pub fn default_toolchain(rustup: &OsStr) -> Result<Option<String>> {
    let output = Command::new(rustup)
        .arg("default")
        .output()
        .map_err(|error| format!("failed to query rustup default: {error}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        if output.status.code() == Some(1)
            && error.trim() == "error: no default toolchain is configured"
        {
            return Ok(None);
        }
        return Err(format!(
            "rustup default query failed ({}): {}",
            output.status,
            error.trim()
        )
        .into());
    }

    let output = String::from_utf8(output.stdout)?;
    let name = output
        .trim()
        .strip_suffix(" (default)")
        .filter(|name| !name.is_empty() && !name.contains(char::is_whitespace))
        .ok_or("unexpected output from rustup default")?;
    Ok(Some(name.to_owned()))
}

pub fn matches_channel(current: Option<&str>, channel: &str) -> bool {
    current.is_some_and(|current| {
        current == channel
            || current
                .strip_prefix(channel)
                .and_then(|suffix| suffix.strip_prefix('-'))
                .is_some_and(|host| !host.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_channel_without_rustups_host_suffix() {
        let channel = "nightly-2026-10-05";
        for current in [channel, "nightly-2026-10-05-x86_64-unknown-linux-gnu"] {
            assert!(matches_channel(Some(current), channel));
        }
        for current in [
            None,
            Some("stable"),
            Some("nightly"),
            Some("nightly-2026-09-05-x86_64-unknown-linux-gnu"),
            Some("nightly-2026-10-050"),
            Some("nightly-2026-10-05-"),
        ] {
            assert!(!matches_channel(current, channel));
        }
    }
}
