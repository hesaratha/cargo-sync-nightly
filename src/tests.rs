use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use super::*;

fn project_args(path: &Path, check: bool) -> Result<Args> {
    let mut args = vec![
        OsString::from("cargo-sync-nightly"),
        OsString::from("--toolchain-file"),
        path.as_os_str().to_owned(),
    ];
    if check {
        args.push(OsString::from("--check"));
    }
    Ok(Args::parse_from(args)?)
}

fn global_args() -> Result<Args> {
    Ok(Args::parse_from(
        ["cargo-sync-nightly", "--check", "--globally"].map(OsString::from),
    )?)
}

fn date() -> String {
    "2026-10-05".to_owned()
}

#[test]
fn project_check_reports_changes_without_writing() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("rust-toolchain.toml");
    let rustup = directory.path().join("no-rustup");
    for (original, expected) in [
        ("[toolchain]\nchannel = 'nightly-2026-10-05' # Keep", 0),
        ("[toolchain]\nchannel = 'stable' # Keep\n", 1),
        ("[toolchain]\nprofile = 'minimal'\n", 1),
        ("# No table\n", 1),
    ] {
        fs::write(&path, original)?;
        fs::File::options()
            .write(true)
            .open(&path)?
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000))?;
        let modified = fs::metadata(&path)?.modified()?;
        let args = project_args(&path, true)?;
        assert_eq!(
            run(&args, || Ok(date()), rustup.as_os_str())?,
            ExitCode::from(expected)
        );
        assert_eq!(fs::read_to_string(&path)?, original);
        assert_eq!(fs::metadata(&path)?.modified()?, modified);
    }
    Ok(())
}

#[test]
fn project_errors_precede_fetch_and_use_error_status() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("rust-toolchain.toml");
    for original in [
        None,
        Some("[toolchain"),
        Some("[toolchain]\nchannel = 123\n"),
    ] {
        if let Some(original) = original {
            fs::write(&path, original)?;
        }
        let args = project_args(&path, true)?;
        let result = run(
            &args,
            || panic!("invalid file must fail before fetch"),
            OsStr::new("no-rustup"),
        );
        assert_eq!(report(result, args.check), ExitCode::from(2));
        if let Some(original) = original {
            assert_eq!(fs::read_to_string(&path)?, original);
        } else {
            assert!(!path.exists());
        }
    }
    Ok(())
}

#[test]
fn fetch_errors_leave_project_unchanged() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("rust-toolchain.toml");
    let original = "[toolchain]\nchannel = 'stable'\n";
    fs::write(&path, original)?;
    for check in [false, true] {
        let args = project_args(&path, check)?;
        let result = run(
            &args,
            || Err("network unavailable".into()),
            OsStr::new("no-rustup"),
        );
        assert_eq!(
            report(result, check),
            ExitCode::from(if check { 2 } else { 1 })
        );
        assert_eq!(fs::read_to_string(&path)?, original);
    }
    Ok(())
}

#[test]
fn normal_project_run_applies_the_prepared_update() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("rust-toolchain.toml");
    fs::write(&path, "[toolchain]\nchannel = 'stable' # Keep\n")?;
    let args = project_args(&path, false)?;
    assert_eq!(
        run(&args, || Ok(date()), OsStr::new("no-rustup"))?,
        ExitCode::SUCCESS
    );
    assert_eq!(
        fs::read_to_string(&path)?,
        "[toolchain]\nchannel = \"nightly-2026-10-05\" # Keep\n"
    );
    Ok(())
}

#[test]
fn missing_rustup_is_an_error() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let args = global_args()?;
    let rustup = directory.path().join("no-rustup");
    assert_eq!(
        report(run(&args, || Ok(date()), rustup.as_os_str()), true),
        ExitCode::from(2)
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn global_check_only_queries_default_and_distinguishes_errors() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir()?;
    let rustup = directory.path().join("rustup");
    let calls = directory.path().join("rustup.calls");
    let args = global_args()?;
    for (response, expected) in [
        (
            "echo 'nightly-2026-10-05-x86_64-unknown-linux-gnu (default)'",
            0,
        ),
        ("echo 'stable-x86_64-unknown-linux-gnu (default)'", 1),
        (
            "echo 'error: no default toolchain is configured' >&2\nexit 1",
            1,
        ),
        ("echo 'error: permission denied' >&2\nexit 1", 2),
        ("echo 'unexpected output'", 2),
        ("exit 0", 2),
    ] {
        fs::write(
            &rustup,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$0.calls\"\n\
             [ \"$#\" -eq 1 ] && [ \"$1\" = default ] || exit 99\n{response}\n"
            ),
        )?;
        fs::set_permissions(&rustup, fs::Permissions::from_mode(0o700))?;
        assert_eq!(
            report(run(&args, || Ok(date()), rustup.as_os_str()), true),
            ExitCode::from(expected)
        );
        assert_eq!(fs::read_to_string(&calls)?, "default\n");
    }
    Ok(())
}
