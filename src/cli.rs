use std::ffi::OsString;
use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "cargo sync-nightly", version, about)]
pub struct Args {
    /// Set global rustup default instead of updating project file
    #[arg(long, conflicts_with_all = ["project", "toolchain_file"])]
    pub globally: bool,

    /// Update nearest rust-toolchain.toml in current directory or ancestors
    /// (default)
    #[arg(long)]
    project: bool,

    /// Update existing file at PATH instead of searching ancestors
    #[arg(long, value_name = "PATH")]
    pub toolchain_file: Option<PathBuf>,

    /// Preview without changes and exit with 0 if current, 1 if changes needed,
    /// 2 on error
    #[arg(long)]
    pub check: bool,
}

impl Args {
    pub fn parse_from(args: impl IntoIterator<Item = OsString>) -> Result<Self, clap::Error> {
        let mut args: Vec<_> = args.into_iter().collect();
        if args.get(1).is_some_and(|arg| arg == "sync-nightly") {
            args.remove(1);
        }
        Self::try_parse_from(args)
    }
}

#[cfg(test)]
mod tests {
    use clap::error::ErrorKind;

    use super::*;

    fn parse(args: &[&str]) -> Result<Args, clap::Error> {
        Args::parse_from(args.iter().map(OsString::from))
    }

    #[test]
    fn selects_project_by_default_or_explicitly() -> crate::Result<()> {
        for prefix in [
            vec!["cargo-sync-nightly"],
            vec!["cargo-sync-nightly", "sync-nightly"],
        ] {
            for flags in [vec![], vec!["--project"]] {
                let args = parse(&[prefix.clone(), flags].concat())?;
                assert!(!args.globally);
                assert!(!args.check);
                assert!(args.toolchain_file.is_none());
            }
        }
        Ok(())
    }

    #[test]
    fn accepts_check_with_each_target() -> crate::Result<()> {
        for flags in [
            vec!["--check"],
            vec!["--check", "--project"],
            vec!["--check", "--globally"],
            vec!["--check", "--toolchain-file", "custom.toml"],
        ] {
            for prefix in [
                vec!["cargo-sync-nightly"],
                vec!["cargo-sync-nightly", "sync-nightly"],
            ] {
                assert!(parse(&[prefix, flags.clone()].concat())?.check);
            }
        }
        Ok(())
    }

    #[test]
    fn selects_global_only_when_requested() -> crate::Result<()> {
        for args in [
            vec!["cargo-sync-nightly", "--globally"],
            vec!["cargo-sync-nightly", "sync-nightly", "--globally"],
        ] {
            let args = parse(&args)?;
            assert!(args.globally);
            assert!(args.toolchain_file.is_none());
        }
        Ok(())
    }

    #[test]
    fn accepts_explicit_file_with_or_without_project() -> crate::Result<()> {
        for flags in [
            vec!["--toolchain-file", "project with spaces/custom.toml"],
            vec![
                "--project",
                "--toolchain-file",
                "project with spaces/custom.toml",
            ],
            vec![
                "--toolchain-file=project with spaces/custom.toml",
                "--project",
            ],
        ] {
            let args = parse(&[vec!["cargo-sync-nightly", "sync-nightly"], flags].concat())?;
            assert!(!args.globally);
            assert_eq!(
                args.toolchain_file,
                Some(PathBuf::from("project with spaces/custom.toml"))
            );
        }
        Ok(())
    }

    #[test]
    fn rejects_conflicting_or_invalid_arguments() {
        for (flags, kind) in [
            (vec!["--globally", "--project"], ErrorKind::ArgumentConflict),
            (vec!["--project", "--globally"], ErrorKind::ArgumentConflict),
            (
                vec!["--globally", "--toolchain-file", "custom.toml"],
                ErrorKind::ArgumentConflict,
            ),
            (
                vec!["--toolchain-file", "custom.toml", "--globally"],
                ErrorKind::ArgumentConflict,
            ),
            (vec!["--toolchain-file"], ErrorKind::InvalidValue),
            (vec!["--unknown"], ErrorKind::UnknownArgument),
            (vec!["custom.toml"], ErrorKind::UnknownArgument),
        ] {
            let error = parse(&[vec!["cargo-sync-nightly"], flags].concat())
                .expect_err("invalid arguments should fail");
            assert_eq!(error.kind(), kind);
        }
    }

    #[test]
    fn handles_help_and_version_for_both_invocations() {
        for prefix in [
            vec!["cargo-sync-nightly"],
            vec!["cargo-sync-nightly", "sync-nightly"],
        ] {
            for (flag, kind) in [
                ("--help", ErrorKind::DisplayHelp),
                ("-h", ErrorKind::DisplayHelp),
                ("--version", ErrorKind::DisplayVersion),
                ("-V", ErrorKind::DisplayVersion),
            ] {
                let error = parse(&[prefix.clone(), vec![flag]].concat())
                    .expect_err("help and version should exit before running");
                assert_eq!(error.kind(), kind);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn accepts_non_utf8_paths() -> crate::Result<()> {
        use std::os::unix::ffi::OsStringExt;

        let path = OsString::from_vec(b"project-\xff/rust-toolchain.toml".to_vec());
        let args = Args::parse_from([
            OsString::from("cargo-sync-nightly"),
            OsString::from("--toolchain-file"),
            path.clone(),
        ])?;
        assert_eq!(args.toolchain_file, Some(PathBuf::from(path)));
        Ok(())
    }
}
