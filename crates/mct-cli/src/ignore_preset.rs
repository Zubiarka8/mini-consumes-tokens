//! Optional generated-directory rules, using the existing .mctignore semantics.
use std::fs;
use std::io::Write;
use std::path::Path;

pub(super) const PATTERNS: &[&str] = &[
    "venv/",
    ".venv/",
    "__pycache__/",
    ".pytest_cache/",
    ".mypy_cache/",
    ".ruff_cache/",
    ".tox/",
    ".nox/",
    "bin/",
    "obj/",
    "build/",
    "dist/",
];

pub(super) fn run(root: &Path, import_gitignore: bool) -> anyhow::Result<()> {
    let root = root.canonicalize()?;
    let path = root.join(mct_index::IGNORE_FILE_NAME);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            anyhow::ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "refusing a non-regular .mctignore"
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                anyhow::ensure!(metadata.nlink() == 1, "refusing hard-linked .mctignore");
            }
            Some(metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let original = if metadata.is_some() {
        fs::read_to_string(&path)?
    } else {
        mct_index::IGNORE_FILE_TEMPLATE.to_string()
    };
    let mut contents = original.clone();
    if import_gitignore
        && !contents
            .lines()
            .any(|line| line.trim() == mct_index::GITIGNORE_IMPORT_DIRECTIVE)
    {
        if !contents.is_empty() && !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(mct_index::GITIGNORE_IMPORT_DIRECTIVE);
        contents.push('\n');
    }
    let missing: Vec<_> = PATTERNS
        .iter()
        .filter(|pattern| !contents.lines().any(|line| line.trim() == **pattern))
        .collect();
    if !missing.is_empty() {
        // Prepend defaults so existing user rules and ! re-includes keep priority.
        let mut block =
            "# Generated directories (editable; bin/ may contain project source).\n".to_string();
        for pattern in missing {
            block.push_str(pattern);
            block.push('\n');
        }
        block.push('\n');
        block.push_str(&contents);
        contents = block;
    }
    if metadata.is_none() || contents != original {
        let mut temp = tempfile::NamedTempFile::new_in(&root)?;
        if let Some(metadata) = metadata {
            temp.as_file().set_permissions(metadata.permissions())?;
        }
        temp.write_all(contents.as_bytes())?;
        temp.as_file_mut().sync_all()?;
        temp.persist(&path).map_err(|error| error.error)?;
    }
    println!("Generated-directory rules configured in {}. Review bin/ and other source directories, then run reindex and restart web.", crate::display_path(&path));
    Ok(())
}
