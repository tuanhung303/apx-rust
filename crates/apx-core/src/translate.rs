use crate::{ChangeKind, ChangeSet};

pub fn translate_apply_patch(changes: &ChangeSet) -> Result<String, String> {
    if changes.changes.is_empty() {
        return Err("script does not change the workspace".to_owned());
    }
    let mut patch = String::from("*** Begin Patch\n");
    for change in &changes.changes {
        match change.kind {
            ChangeKind::Add => {
                let path = slash(change.path.as_deref().unwrap_or_default());
                patch.push_str(&format!("*** Add File: {path}\n"));
                let content = normalize(&change.content);
                if content.is_empty() || !content.ends_with('\n') {
                    return Err(format!(
                        "translating new file {path} requires LF-terminated content"
                    ));
                }
                for line in content.strip_suffix('\n').expect("checked").split('\n') {
                    patch.push('+');
                    patch.push_str(line);
                    patch.push('\n');
                }
            }
            ChangeKind::Delete => {
                patch.push_str(&format!(
                    "*** Delete File: {}\n",
                    slash(change.original_path.as_deref().unwrap_or_default())
                ));
            }
            ChangeKind::Update | ChangeKind::Move => {
                patch.push_str(&format!(
                    "*** Update File: {}\n",
                    slash(change.original_path.as_deref().unwrap_or_default())
                ));
                if change.original_path != change.path {
                    patch.push_str(&format!(
                        "*** Move to: {}\n",
                        slash(change.path.as_deref().unwrap_or_default())
                    ));
                }
                patch.push_str("@@\n");
                if change.original == change.content {
                    let first = normalize(&change.original)
                        .split('\n')
                        .next()
                        .unwrap_or("")
                        .to_owned();
                    if first.is_empty() {
                        patch.push_str("-\n+\n");
                    } else {
                        patch.push(' ');
                        patch.push_str(&first);
                        patch.push('\n');
                    }
                } else {
                    for line in normalized_lines(&change.original) {
                        patch.push('-');
                        patch.push_str(&line);
                        patch.push('\n');
                    }
                    for line in normalized_lines(&change.content) {
                        patch.push('+');
                        patch.push_str(&line);
                        patch.push('\n');
                    }
                }
            }
        }
    }
    patch.push_str("*** End Patch\n");
    Ok(patch)
}

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn normalized_lines(text: &str) -> Vec<String> {
    let normalized = normalize(text);
    let mut lines: Vec<String> = normalized.split('\n').map(str::to_owned).collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

fn slash(path: &str) -> String {
    path.replace('\\', "/")
}
