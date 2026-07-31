#![forbid(unsafe_code)]
//! Capability-safe local filesystem and transactions.
//!
//! This crate owns everything that touches the disk: a [`Baseline`] that reads
//! workspace files relative to a canonical root, script-path resolution against
//! `--cwd`/`--root`, and the all-or-nothing commit of a change set.

use apx_core::{Baseline, Change, ChangeKind, ChangeSet, Instruction, Operation, Program};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Canonicalize `root` and return `(canonical, alias)` where `alias` is the
/// caller-supplied spelling when it differs (mirrors Go's root alias handling).
pub fn canonicalize_root(root: &Path) -> Result<(PathBuf, PathBuf), String> {
    let alias = lexical_clean(root);
    let canonical = fs::canonicalize(&alias)
        .map_err(|error| format!("canonicalizing workspace root: {error}"))?;
    Ok((canonical, alias))
}

/// Resolve every script path against `cwd` (root-relative, default `.`) or the
/// absolute root/alias, producing a program whose paths are root-relative.
/// Paths that escape the root are rejected before evaluation.
pub fn resolve_paths(
    program: Program,
    canonical_root: &Path,
    alias: &Path,
    cwd: &str,
) -> Result<Program, String> {
    let mut instructions = Vec::with_capacity(program.instructions.len());
    for instruction in program.instructions {
        let operation = match &instruction.operation {
            Operation::In { path } => Operation::In {
                path: resolve_path(path, canonical_root, alias, cwd)?,
            },
            Operation::New { path } => Operation::New {
                path: resolve_path(path, canonical_root, alias, cwd)?,
            },
            Operation::Move { path } => Operation::Move {
                path: resolve_path(path, canonical_root, alias, cwd)?,
            },
            Operation::Remove { path: Some(path) } => Operation::Remove {
                path: Some(resolve_path(path, canonical_root, alias, cwd)?),
            },
            other => other.clone(),
        };
        instructions.push(Instruction {
            line: instruction.line,
            operation,
        });
    }
    Ok(Program { instructions })
}

fn resolve_path(
    path: &str,
    canonical_root: &Path,
    alias: &Path,
    cwd: &str,
) -> Result<String, String> {
    let relative = if path.starts_with('/') {
        strip_root_prefix(path, canonical_root)
            .or_else(|| strip_root_prefix(path, alias))
            .ok_or_else(|| "path resolves outside workspace root".to_owned())?
    } else {
        let joined = if cwd.is_empty() || cwd == "." {
            path.to_owned()
        } else {
            format!("{cwd}/{path}")
        };
        let cleaned = apx_core::clean_path(&joined);
        if cleaned == ".." || cleaned.starts_with("../") {
            return Err("path resolves outside workspace root".to_owned());
        }
        cleaned
    };
    if relative.is_empty() {
        return Err("path resolves outside workspace root".to_owned());
    }
    Ok(relative)
}

/// Strip `root`'s cleaned prefix from a cleaned absolute path, returning the
/// root-relative remainder, or `None` when `path` is not under `root`.
/// Both sides are compared via `clean_path`, so the leading root component is
/// never confused with `Path::components()`'s `RootDir` entry.
pub fn strip_root_prefix(path: &str, root: &Path) -> Option<String> {
    let cleaned = apx_core::clean_path(path);
    let root_cleaned = apx_core::clean_path(&root.to_string_lossy());
    let mut path_components = cleaned
        .split('/')
        .filter(|part| !part.is_empty())
        .peekable();
    let mut root_components = root_cleaned
        .split('/')
        .filter(|part| !part.is_empty())
        .peekable();
    loop {
        match (path_components.peek(), root_components.next()) {
            (Some(&part), Some(component)) => {
                if part != component {
                    return None;
                }
                path_components.next();
            }
            (Some(_), None) => {
                let rest = path_components.collect::<Vec<_>>().join("/");
                return Some(rest);
            }
            (None, Some(_)) => return None,
            (None, None) => return Some(String::new()),
        }
    }
}

fn lexical_clean(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    PathBuf::from(apx_core::clean_path(&text))
}

/// Filesystem-backed baseline rooted at a canonical directory.
pub struct FsBaseline {
    root: PathBuf,
}

impl FsBaseline {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn resolve(&self, path: &str) -> Result<PathBuf, String> {
        let target = lexical_clean(&self.root.join(path));
        if !target.starts_with(&self.root) {
            return Err(format!(
                "reading {path}: path resolves outside workspace root"
            ));
        }
        Ok(target)
    }
}

impl Baseline for FsBaseline {
    fn read(&self, path: &str) -> Result<Option<String>, String> {
        let target = self.resolve(path)?;
        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("reading {path}: {error}")),
        };
        if metadata.file_type().is_symlink() {
            let canonical =
                fs::canonicalize(&target).map_err(|error| format!("reading {path}: {error}"))?;
            if !canonical.starts_with(&self.root) {
                return Err(format!(
                    "reading {path}: path resolves outside workspace root"
                ));
            }
        }
        if !metadata.is_file() {
            return Err(format!("reading {path}: not a regular file"));
        }
        let bytes = fs::read(&target).map_err(|error| format!("reading {path}: {error}"))?;
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| format!("reading {path}: not UTF-8"))
    }
}

struct Staged {
    change: Change,
    temp: Option<PathBuf>,
    backup: Option<PathBuf>,
    installed: bool,
    backed_up: bool,
}

/// Apply a change set atomically: stage every output, back up every original,
/// install outputs, then delete backups. Any failure rolls back to the exact
/// pre-call state and reports an error naming the failing action.
pub fn apply(root: &Path, changes: &ChangeSet) -> Result<(), String> {
    let mut staged: Vec<Staged> = Vec::with_capacity(changes.changes.len());
    let mut created_dirs: Vec<PathBuf> = Vec::new();

    for change in &changes.changes {
        match change.kind {
            ChangeKind::Add => {
                let path = join_checked(root, change.path.as_deref().unwrap_or_default())?;
                ensure_no_symlink_final(&path)?;
                let parent = path
                    .parent()
                    .ok_or_else(|| "new path has no parent".to_owned())?;
                if !parent.starts_with(root) {
                    return Err("path resolves outside workspace root".to_owned());
                }
                create_parents(parent, &mut created_dirs)?;
                let temp = write_temp(parent, &change.content)?;
                staged.push(Staged {
                    change: change.clone(),
                    temp: Some(temp),
                    backup: None,
                    installed: false,
                    backed_up: false,
                });
            }
            ChangeKind::Update | ChangeKind::Move => {
                let original =
                    join_checked(root, change.original_path.as_deref().unwrap_or_default())?;
                ensure_no_symlink_final(&original)?;
                let original_bytes = fs::read(&original).map_err(|error| {
                    format!(
                        "changing {}: {error}",
                        change.original_path.as_deref().unwrap_or_default()
                    )
                })?;
                let expected = change.original.as_bytes();
                if original_bytes != expected {
                    return Err(format!(
                        "changing {}: file changed since baseline",
                        change.original_path.as_deref().unwrap_or_default()
                    ));
                }
                let parent = original
                    .parent()
                    .ok_or_else(|| "original path has no parent".to_owned())?;
                let backup = unique_temp_name(parent, "backup");
                fs::copy(&original, &backup).map_err(|error| {
                    format!(
                        "backing up {}: {error}",
                        change.original_path.as_deref().unwrap_or_default()
                    )
                })?;
                let target = join_checked(root, change.path.as_deref().unwrap_or_default())?;
                ensure_no_symlink_final(&target)?;
                let target_parent = target
                    .parent()
                    .ok_or_else(|| "path has no parent".to_owned())?;
                create_parents(target_parent, &mut created_dirs)?;
                let temp = write_temp(target_parent, &change.content)?;
                staged.push(Staged {
                    change: change.clone(),
                    temp: Some(temp),
                    backup: Some(backup),
                    installed: false,
                    backed_up: false,
                });
            }
            ChangeKind::Delete => {
                let original =
                    join_checked(root, change.original_path.as_deref().unwrap_or_default())?;
                ensure_no_symlink_final(&original)?;
                let parent = original
                    .parent()
                    .ok_or_else(|| "original path has no parent".to_owned())?;
                let backup = unique_temp_name(parent, "backup");
                fs::copy(&original, &backup).map_err(|error| {
                    format!(
                        "backing up {}: {error}",
                        change.original_path.as_deref().unwrap_or_default()
                    )
                })?;
                staged.push(Staged {
                    change: change.clone(),
                    temp: None,
                    backup: Some(backup),
                    installed: false,
                    backed_up: false,
                });
            }
        }
    }

    for index in 0..staged.len() {
        if staged[index].change.kind == ChangeKind::Add {
            continue;
        }
        let original = join_checked(
            root,
            staged[index]
                .change
                .original_path
                .as_deref()
                .unwrap_or_default(),
        )
        .map_err(|error| fail_apply(&staged, root, &created_dirs, "backing up", error))?;
        let backup = staged[index]
            .backup
            .as_ref()
            .expect("non-add change has backup")
            .clone();
        if let Err(error) = fs::rename(&original, &backup) {
            return Err(fail_apply(
                &staged,
                root,
                &created_dirs,
                "backing up",
                error.to_string(),
            ));
        }
        staged[index].backed_up = true;
    }

    for index in 0..staged.len() {
        if staged[index].change.kind == ChangeKind::Delete {
            continue;
        }
        let target = join_checked(
            root,
            staged[index].change.path.as_deref().unwrap_or_default(),
        )
        .map_err(|error| fail_apply(&staged, root, &created_dirs, "installing", error))?;
        let temp = staged[index]
            .temp
            .as_ref()
            .expect("non-delete change has temp")
            .clone();
        if let Err(error) = fs::rename(&temp, &target) {
            return Err(fail_apply(
                &staged,
                root,
                &created_dirs,
                "installing",
                error.to_string(),
            ));
        }
        staged[index].installed = true;
    }

    for item in &mut staged {
        if let Some(backup) = item.backup.clone()
            && item.backed_up
        {
            let _ = fs::remove_file(&backup);
            item.backed_up = false;
        }
    }

    cleanup_staging(&staged, &created_dirs)?;
    Ok(())
}

fn fail_apply(
    staged: &[Staged],
    root: &Path,
    created_dirs: &[PathBuf],
    action: &str,
    cause: String,
) -> String {
    let rolled_back = rollback(staged, root);
    let cleaned = cleanup_staging(staged, created_dirs);
    let mut message = format!("{action}: {cause}");
    if let Err(rollback_error) = rolled_back {
        message.push_str(&format!("; rollback: {rollback_error}"));
    }
    if let Err(cleanup_error) = cleaned {
        message.push_str(&format!("; cleanup: {cleanup_error}"));
    }
    message
}

fn join_checked(root: &Path, path: &str) -> Result<PathBuf, String> {
    let target = lexical_clean(&root.join(path));
    if !target.starts_with(root) {
        return Err("path resolves outside workspace root".to_owned());
    }
    Ok(target)
}

fn ensure_no_symlink_final(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "writing to a final-component symbolic link is not supported: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspecting {}: {error}", path.display())),
    }
}

fn create_parents(parent: &Path, created: &mut Vec<PathBuf>) -> Result<(), String> {
    if parent.exists() {
        if !parent.is_dir() {
            return Err(format!("{} is not a directory", parent.display()));
        }
        return Ok(());
    }
    let mut missing = Vec::new();
    let mut current = Some(parent);
    while let Some(directory) = current {
        if directory.exists() {
            break;
        }
        missing.push(directory.to_owned());
        current = directory.parent();
    }
    fs::create_dir_all(parent)
        .map_err(|error| format!("creating directory {}: {error}", parent.display()))?;
    created.extend(missing);
    Ok(())
}

fn write_temp(directory: &Path, content: &str) -> Result<PathBuf, String> {
    let name = unique_temp_name(directory, "stage");
    let mut file = fs::File::create(&name).map_err(|error| {
        format!(
            "creating temporary file in {}: {error}",
            directory.display()
        )
    })?;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("writing temporary file: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("syncing temporary file: {error}"))?;
    Ok(name)
}

fn unique_temp_name(directory: &Path, kind: &str) -> PathBuf {
    loop {
        let suffix: String = std::iter::repeat_with(fast_random_byte)
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let candidate = directory.join(format!(".{kind}-{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
}

fn fast_random_byte() -> u8 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    (nanos ^ (nanos >> 32)) as u8
}

fn rollback(staged: &[Staged], root: &Path) -> Result<(), String> {
    let mut errors = Vec::new();
    for item in staged.iter().rev() {
        if item.installed {
            let target = join_checked(root, item.change.path.as_deref().unwrap_or_default());
            if let Ok(target) = target
                && let Err(error) = fs::remove_file(&target)
            {
                errors.push(error.to_string());
            }
        }
        if item.backed_up
            && let Some(backup) = &item.backup
        {
            let original = join_checked(
                root,
                item.change.original_path.as_deref().unwrap_or_default(),
            );
            if let Ok(original) = original
                && let Err(error) = fs::rename(backup, original)
            {
                errors.push(error.to_string());
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn cleanup_staging(staged: &[Staged], created_dirs: &[PathBuf]) -> Result<(), String> {
    let mut errors: Vec<String> = Vec::new();
    for item in staged {
        if let Some(temp) = &item.temp
            && let Err(error) = fs::remove_file(temp)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            errors.push(error.to_string());
        }
        if let Some(backup) = &item.backup
            && let Err(error) = fs::remove_file(backup)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            errors.push(error.to_string());
        }
    }
    for directory in created_dirs.iter().rev() {
        if let Err(error) = fs::remove_dir(directory)
            && !matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            )
        {
            errors.push(error.to_string());
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Structural equality used by tests and the CLI smoke suite.
pub fn change_paths(changes: &ChangeSet) -> Vec<(String, String)> {
    changes
        .changes
        .iter()
        .map(|change| {
            (
                change.original_path.clone().unwrap_or_default(),
                change.path.clone().unwrap_or_default(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("apx-local-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_paths_joins_cwd() {
        let root = PathBuf::from("/tmp/root");
        let alias = root.clone();
        let program = Program {
            instructions: vec![Instruction {
                line: 1,
                operation: Operation::In {
                    path: "sub/file.go".to_owned(),
                },
            }],
        };
        let resolved = resolve_paths(program, &root, &alias, "a/b").unwrap();
        assert_eq!(
            resolved.instructions[0].operation,
            Operation::In {
                path: "a/b/sub/file.go".to_owned()
            }
        );
    }

    #[test]
    fn resolve_paths_rejects_escape() {
        let root = PathBuf::from("/tmp/root");
        let alias = root.clone();
        let program = Program {
            instructions: vec![Instruction {
                line: 1,
                operation: Operation::In {
                    path: "../../etc/passwd".to_owned(),
                },
            }],
        };
        assert!(resolve_paths(program, &root, &alias, "x").is_err());
    }

    #[test]
    fn strip_root_prefix_matches_absolute_paths() {
        let root = PathBuf::from("/tmp/apx-root");
        assert_eq!(
            strip_root_prefix("/tmp/apx-root", &root),
            Some(String::new())
        );
        assert_eq!(
            strip_root_prefix("/tmp/apx-root/sub/file.go", &root),
            Some("sub/file.go".to_owned())
        );
        assert_eq!(strip_root_prefix("/tmp/apx-rooted", &root), None);
        assert_eq!(strip_root_prefix("/tmp/other", &root), None);
    }

    #[test]
    fn apply_update_and_add_transactionally() {
        let dir = scratch("apply");
        fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        let changes = ChangeSet {
            changes: vec![
                Change {
                    kind: ChangeKind::Update,
                    original_path: Some("a.txt".to_owned()),
                    path: Some("a.txt".to_owned()),
                    original: "one\ntwo\n".to_owned(),
                    content: "one\nthree\n".to_owned(),
                },
                Change {
                    kind: ChangeKind::Add,
                    original_path: None,
                    path: Some("nested/b.txt".to_owned()),
                    original: String::new(),
                    content: "hello\n".to_owned(),
                },
            ],
        };
        apply(&dir, &changes).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("a.txt")).unwrap(),
            "one\nthree\n"
        );
        assert_eq!(
            fs::read_to_string(dir.join("nested/b.txt")).unwrap(),
            "hello\n"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn apply_move_removes_original() {
        let dir = scratch("move");
        fs::write(dir.join("old.txt"), "payload\n").unwrap();
        let changes = ChangeSet {
            changes: vec![Change {
                kind: ChangeKind::Move,
                original_path: Some("old.txt".to_owned()),
                path: Some("sub/new.txt".to_owned()),
                original: "payload\n".to_owned(),
                content: "payload\n".to_owned(),
            }],
        };
        apply(&dir, &changes).unwrap();
        assert!(!dir.join("old.txt").exists());
        assert_eq!(
            fs::read_to_string(dir.join("sub/new.txt")).unwrap(),
            "payload\n"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn apply_rejects_stale_baseline_and_rolls_back() {
        let dir = scratch("stale");
        fs::write(dir.join("a.txt"), "one\n").unwrap();
        let changes = ChangeSet {
            changes: vec![
                Change {
                    kind: ChangeKind::Update,
                    original_path: Some("a.txt".to_owned()),
                    path: Some("a.txt".to_owned()),
                    original: "stale\n".to_owned(),
                    content: "new\n".to_owned(),
                },
                Change {
                    kind: ChangeKind::Add,
                    original_path: None,
                    path: Some("nested/c.txt".to_owned()),
                    original: String::new(),
                    content: "x\n".to_owned(),
                },
            ],
        };
        assert!(apply(&dir, &changes).is_err());
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one\n");
        assert!(!dir.join("nested").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
