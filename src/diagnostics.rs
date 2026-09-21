use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    process,
    time::{SystemTime, UNIX_EPOCH},
};

// Keep a small local lifecycle log so failures from a graphical launcher,
// which often has no visible stderr, can be diagnosed.
pub fn record(event: &str) {
    let Some(home) = env::var_os("HOME") else {
        return;
    };
    let directory = PathBuf::from(home).join(".local/state/hafthi");
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    let path = directory.join("diagnostics.log");
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() > 64 * 1024) {
        let _ = fs::write(&path, "");
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let _ = writeln!(file, "{timestamp} pid={} {event}", process::id());
}
