use std::{fs, path::Path, time::Duration};

pub fn has_running_task() -> bool {
    let state = std::env::var_os("HAFTHI_GHOST_DIR")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_STATE_HOME")
                .filter(|value| !value.is_empty())
                .map(|path| std::path::PathBuf::from(path).join("hafthi/ghost-tasks"))
        })
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| std::path::PathBuf::from(home).join(".local/state/hafthi/ghost-tasks"))
        });
    state.is_some_and(|path| running_in(&path))
}

fn running_in(state: &Path) -> bool {
    let Ok(tasks) = fs::read_dir(state) else { return false };
    tasks.flatten().any(|entry| {
        if !entry.file_name().to_string_lossy().starts_with("job.") {
            return false;
        }
        let path = entry.path();
        if !path.is_dir() || path.join("exit").exists() {
            return false;
        }
        if let Ok(pid) = fs::read_to_string(path.join("pid")) {
            let Ok(pid) = pid.trim().parse::<u32>() else { return false };
            return pid > 0 && Path::new("/proc").join(pid.to_string()).exists();
        }
        // g creates the directory before its worker writes the PID.
        entry.metadata().ok()
            .and_then(|info| info.modified().ok())
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age < Duration::from_secs(5))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn running_task_disappears_when_exit_is_written() {
        let state = std::env::temp_dir().join(format!("hafthi-ghost-indicator-{}", std::process::id()));
        let task = state.join("job.TEST");
        fs::create_dir_all(&task).unwrap();
        fs::write(task.join("pid"), std::process::id().to_string()).unwrap();
        assert!(running_in(&state));
        fs::write(task.join("exit"), "0").unwrap();
        assert!(!running_in(&state));
        fs::remove_dir_all(state).unwrap();
    }
}
