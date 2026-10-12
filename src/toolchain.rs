use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::Result;

pub fn resolve_path(start: &Path, explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(start.join(path));
    }

    for directory in start.ancestors() {
        let candidate = directory.join("rust-toolchain.toml");
        match fs::symlink_metadata(&candidate) {
            Ok(_) => return Ok(candidate),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("could not inspect {}: {error}", candidate.display()).into());
            }
        }
    }

    Err("could not find rust-toolchain.toml in this directory or any parent\n\
         use --toolchain-file PATH to select an existing file or --globally to set the global default"
        .into())
}

#[derive(Debug)]
pub struct ToolchainFile {
    path: PathBuf,
    document: DocumentMut,
}

impl ToolchainFile {
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let document = content
            .parse::<DocumentMut>()
            .map_err(|error| format!("invalid TOML in {}: {error}", path.display()))?;

        if let Some(toolchain) = document.get("toolchain") {
            let table = toolchain.as_table_like().ok_or_else(|| {
                format!("invalid toolchain in {}: expected a table", path.display())
            })?;
            if table
                .get("channel")
                .is_some_and(|channel| !channel.is_str())
            {
                return Err(format!(
                    "invalid toolchain.channel in {}: expected a string",
                    path.display()
                )
                .into());
            }
        }

        Ok(Self {
            path: path.to_owned(),
            document,
        })
    }

    pub fn pin(mut self, channel: &str) -> Result<bool> {
        let mut new_table = Table::new();
        if self.document.is_empty() {
            let mut prefix = self
                .document
                .trailing()
                .as_str()
                .unwrap_or_default()
                .to_owned();
            if !prefix.is_empty() && !prefix.ends_with('\n') {
                prefix.push('\n');
            }
            new_table.decor_mut().set_prefix(prefix);
            self.document.set_trailing("");
        }

        let table = self
            .document
            .entry("toolchain")
            .or_insert(Item::Table(new_table))
            .as_table_like_mut()
            .ok_or("toolchain must be a table")?;

        if table.get("channel").and_then(Item::as_str) == Some(channel) {
            return Ok(false);
        }

        let mut value = Value::from(channel);
        if let Some(previous) = table.get("channel").and_then(Item::as_value) {
            *value.decor_mut() = previous.decor().clone();
        }
        if let Some(previous) = table.get_mut("channel") {
            *previous = Item::Value(value);
        } else {
            table.insert("channel", Item::Value(value));
        }

        fs::write(&self.path, self.document.to_string())
            .map_err(|error| format!("could not write {}: {error}", self.path.display()))?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANNEL: &str = "nightly-2026-10-05";

    #[test]
    fn finds_nearest_file_in_current_directory_or_ancestors() -> Result<()> {
        let root = tempfile::tempdir()?;
        let parent = root.path().join("workspace");
        let nested = parent.join("crate/src");
        fs::create_dir_all(&nested)?;
        fs::write(root.path().join("rust-toolchain.toml"), "")?;
        let expected = parent.join("rust-toolchain.toml");
        fs::write(&expected, "")?;

        assert_eq!(resolve_path(&nested, None)?, expected);
        assert_eq!(resolve_path(&parent, None)?, expected);
        Ok(())
    }

    #[test]
    fn reports_missing_file() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let error = resolve_path(directory.path(), None).expect_err("missing file should fail");
        assert!(
            error
                .to_string()
                .contains("could not find rust-toolchain.toml")
        );
        Ok(())
    }

    #[test]
    fn does_not_skip_an_invalid_nearest_file() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let nested = directory.path().join("project");
        fs::create_dir(&nested)?;
        fs::write(
            directory.path().join("rust-toolchain.toml"),
            "[toolchain]\n",
        )?;
        let path = nested.join("rust-toolchain.toml");
        fs::write(&path, "invalid TOML")?;

        assert_eq!(resolve_path(&nested, None)?, path);
        assert!(ToolchainFile::load(&path).is_err());
        Ok(())
    }

    #[test]
    fn resolves_explicit_paths_without_searching() -> Result<()> {
        let directory = tempfile::tempdir()?;
        fs::write(directory.path().join("rust-toolchain.toml"), "")?;
        let relative = Path::new("elsewhere/custom.toml");
        let absolute = directory.path().join(relative);

        assert_eq!(resolve_path(directory.path(), Some(relative))?, absolute);
        assert_eq!(resolve_path(directory.path(), Some(&absolute))?, absolute);
        assert!(ToolchainFile::load(&absolute).is_err());
        assert!(!absolute.exists());
        assert!(ToolchainFile::load(directory.path()).is_err());
        Ok(())
    }

    #[test]
    fn preserves_comments_and_unrelated_settings() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("rust-toolchain.toml");
        let original = "# Project pin\n\
                        [toolchain] # Settings\n\
                        # Release cycle\n\
                        channel  =  \"nightly-old\"  # Keep this comment\n\
                        components = [\"rustfmt\", \"clippy\"]\n\
                        targets = [\"wasm32-unknown-unknown\"]\n\
                        profile = 'minimal'\n";
        fs::write(&path, original)?;

        assert!(ToolchainFile::load(&path)?.pin(CHANNEL)?);
        assert_eq!(
            fs::read_to_string(&path)?,
            original.replace("nightly-old", CHANNEL)
        );
        Ok(())
    }

    #[test]
    fn updates_inline_tables_without_reformatting_other_values() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("rust-toolchain.toml");
        let original = "toolchain = { channel = \"nightly-old\", profile = 'minimal' } # Keep\n";
        fs::write(&path, original)?;

        assert!(ToolchainFile::load(&path)?.pin(CHANNEL)?);
        assert_eq!(
            fs::read_to_string(&path)?,
            original.replace("nightly-old", CHANNEL)
        );
        Ok(())
    }

    #[test]
    fn adds_missing_table_or_channel() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("rust-toolchain.toml");
        for original in [
            "# Empty file\n",
            "# Empty file",
            "[toolchain]\nprofile = 'minimal'\n",
            "toolchain = { profile = 'minimal' }\n",
        ] {
            fs::write(&path, original)?;
            assert!(ToolchainFile::load(&path)?.pin(CHANNEL)?);
            let updated = fs::read_to_string(&path)?;
            let document = updated.parse::<DocumentMut>()?;
            assert_eq!(document["toolchain"]["channel"].as_str(), Some(CHANNEL));
            if original.contains("profile") {
                assert_eq!(document["toolchain"]["profile"].as_str(), Some("minimal"));
            } else {
                assert!(updated.starts_with("# Empty file\n"));
            }
        }
        Ok(())
    }

    #[test]
    fn rejects_invalid_documents_without_writing() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("rust-toolchain.toml");
        for original in [
            "[toolchain",
            "toolchain = 'nightly'\n",
            "[[toolchain]]\nchannel = 'nightly'\n",
            "[toolchain]\nchannel = 123\n",
            "[toolchain.channel]\nvalue = 'nightly'\n",
        ] {
            fs::write(&path, original)?;
            let error = ToolchainFile::load(&path).expect_err("invalid document should fail");
            assert!(error.to_string().contains(&path.display().to_string()));
            assert_eq!(fs::read_to_string(&path)?, original);
        }
        Ok(())
    }

    #[test]
    fn does_not_rewrite_an_unchanged_channel() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("rust-toolchain.toml");
        let original = format!("[toolchain]\nchannel = '{CHANNEL}' # Keep");
        fs::write(&path, &original)?;
        let timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        fs::File::options()
            .write(true)
            .open(&path)?
            .set_modified(timestamp)?;
        let modified = fs::metadata(&path)?.modified()?;

        assert!(!ToolchainFile::load(&path)?.pin(CHANNEL)?);
        assert_eq!(fs::read_to_string(&path)?, original);
        assert_eq!(fs::metadata(&path)?.modified()?, modified);
        Ok(())
    }
}
