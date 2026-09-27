//! Running things: the "lamppost" script, editors, browsers, terminals.
use crate::settings::{path, which};
use std::process::{Command, Stdio};

/// Result of a "lamppost start/stop" run, sent back to the UI thread.
pub struct Done {
    pub action: &'static str,
    pub keys: Vec<String>,
    pub output: String,
    pub code: i32,
}

/// Start or stop modules in the background; the UI keeps repainting meanwhile.
pub fn run(action: &'static str, keys: Vec<String>, reply: impl FnOnce(Done) + Send + 'static) {
    std::thread::spawn(move || {
        let output = Command::new(path("lamppost"))
            .arg(action)
            .args(&keys)
            .stdin(Stdio::null())
            .output();
        let done = match output {
            Ok(out) => Done {
                action,
                keys,
                output: String::from_utf8_lossy(&out.stdout).to_string()
                    + &String::from_utf8_lossy(&out.stderr),
                code: out.status.code().unwrap_or(-1),
            },
            Err(e) => Done {
                action,
                keys,
                output: format!("could not run {}: {e}", path("lamppost").display()),
                code: -1,
            },
        };
        reply(done);
    });
}

pub fn detached(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

pub fn open_url(url: &str) -> bool {
    detached("xdg-open", &[url])
}

pub fn open_path(target: &std::path::Path) -> bool {
    detached("xdg-open", &[&target.to_string_lossy()])
}

pub fn open_in_editor(editor: &str, target: &std::path::Path) -> bool {
    let mut parts = editor.split_whitespace();
    let Some(program) = parts.next() else { return false };
    let mut args: Vec<String> = parts.map(str::to_string).collect();
    args.push(target.to_string_lossy().to_string());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    detached(program, &args)
}

pub fn open_terminal(dir: &std::path::Path) -> bool {
    let dir = dir.to_string_lossy().to_string();
    let candidates: [(&str, Vec<String>); 4] = [
        ("ptyxis", vec!["--new-window".into(), "--working-directory".into(), dir.clone()]),
        ("gnome-terminal", vec![format!("--working-directory={dir}")]),
        ("konsole", vec!["--workdir".into(), dir.clone()]),
        ("xterm", vec![]),
    ];
    for (program, args) in candidates {
        if which(program).is_some() {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            if detached(program, &args) {
                return true;
            }
        }
    }
    false
}

/// Newest file matching a pattern that may contain "*".
pub fn resolve_glob(pattern: &std::path::Path) -> Option<std::path::PathBuf> {
    let text = pattern.to_string_lossy().to_string();
    if !text.contains('*') {
        return pattern.exists().then(|| pattern.to_path_buf());
    }
    let (dir, name) = (pattern.parent()?, pattern.file_name()?.to_string_lossy().to_string());
    let (before, after) = name.split_once('*')?;
    let mut best: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let file = entry.file_name().to_string_lossy().to_string();
        if file.starts_with(before) && file.ends_with(after) {
            let time = entry.metadata().and_then(|m| m.modified()).ok()?;
            if best.as_ref().map(|(t, _)| time > *t).unwrap_or(true) {
                best = Some((time, entry.path()));
            }
        }
    }
    best.map(|(_, p)| p)
}
