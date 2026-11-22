//! Staff audit log: one JSON line per staff action, appended to `audit.log`
//! in the data folder (owner-readable only). Lines are never rewritten.

use serde_json::json;
use std::{
    fs,
    io::{self, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// Appends one entry. `actor` is the staff username, `target` who or what was acted on.
pub fn record(data_dir: &Path, actor: &str, action: &str, target: &str) -> io::Result<()> {
    fs::create_dir_all(data_dir)?;
    let mut options = fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // serde_json escapes newlines, so one entry is always exactly one line.
    let line = format!(
        "{}\n",
        json!({"at": at, "actor": actor, "action": action, "target": target})
    );
    let mut file = options.open(data_dir.join("audit.log"))?;
    file.write_all(line.as_bytes())?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_append_one_line_each_even_with_newlines_in_a_name() {
        let dir = tempfile::tempdir().unwrap();
        record(dir.path(), "mod", "mute", "a\nb").unwrap();
        record(dir.path(), "mod", "kick", "c").unwrap();
        let text = fs::read_to_string(dir.path().join("audit.log")).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["target"], "a\nb");
        assert_eq!(lines[1]["action"], "kick");
    }
}
