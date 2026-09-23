use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

use crate::diagnostics;
use crate::settings::Settings;
use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use winit::event_loop::EventLoopProxy;

#[derive(Debug)]
pub enum AppEvent {
    PtyOutput(Vec<u8>),
    PtyExited,
    ImageChosen(Option<String>),
}

// fish 4 queries Primary Device Attributes with CSI 0 c on startup.
// The query may be split across PTY reads, so retain a small parser state.
#[derive(Default)]
struct PrimaryDeviceQuery {
    state: u8,
}

impl PrimaryDeviceQuery {
    fn observe(&mut self, bytes: &[u8]) -> usize {
        let mut queries = 0;
        for &byte in bytes {
            self.state = match (self.state, byte) {
                (_, 0x1b) => 1,
                (1, b'[') => 2,
                (2, b'0') => 3,
                (2, b'c') | (3, b'c') => {
                    queries += 1;
                    0
                }
                _ => 0,
            };
        }
        queries
    }
}

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

pub fn installed_program(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates = vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin"),
        PathBuf::from("/usr/local/bin")];
    if let Some(home) = home {
        candidates.push(home.join(".local/bin"));
        candidates.push(home.join(".cargo/bin"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path));
    }
    candidates.into_iter().map(|dir| dir.join(name)).find(|path| {
        std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
            && is_executable(path)
    })
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

fn fish_quote(path: &Path) -> String {
    let path = path.to_string_lossy();
    format!("'{}'", path.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn fish_init(settings: &Settings, starship: Option<&Path>) -> String {
    let mut commands = Vec::new();
    if !settings.show_fish_greeting {
        commands.push("function fish_greeting; end".to_owned());
    }
    if settings.use_starship {
        if let Some(starship) = starship {
            // -C runs after config.fish. Avoid initializing Starship twice if
            // the user's own Fish configuration has already done so.
            commands.push(format!(
                "if not functions -q __starship_set_job_count; {} init fish | source; end",
                fish_quote(starship)
            ));
        }
    }
    commands.join("; ")
}

fn shell_command(settings: &Settings) -> CommandBuilder {
    let fish = settings.use_fish.then(|| installed_program("fish")).flatten();
    // A graphical launcher may omit SHELL. portable-pty resolves the account
    // shell when Fish is absent or the user disables it in Preferences.
    let mut cmd = if let Some(fish) = fish {
        let mut cmd = CommandBuilder::new(fish);
        cmd.arg("-l");
        let init = fish_init(settings, installed_program("starship").as_deref());
        if !init.is_empty() {
            cmd.arg("-C");
            cmd.arg(init);
        }
        cmd
    } else {
        CommandBuilder::new_default_prog()
    };
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "Hafthi");
    cmd.env("HAFTHI", "1");
    cmd
}

impl PtySession {
    pub fn spawn(cols: u16, rows: u16, proxy: EventLoopProxy<AppEvent>, settings: &Settings) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to create PTY")?;

        let mut child = pair
            .slave
            .spawn_command(shell_command(settings))
            .context("failed to spawn shell in PTY")?;

        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone PTY reader")?;
        let writer = Arc::new(Mutex::new(
            pair.master
                .take_writer()
                .context("failed to open PTY writer")?,
        ));

        let reader_proxy = proxy.clone();
        let query_writer = Arc::clone(&writer);
        thread::spawn(move || {
            let mut primary_device_query = PrimaryDeviceQuery::default();
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        for _ in 0..primary_device_query.observe(&buf[..n]) {
                            // Conservative VT100 DA1: no optional terminal capabilities.
                            if let Ok(mut output) = query_writer.lock() {
                                let _ = output.write_all(b"\x1b[?1;0c");
                                let _ = output.flush();
                            }
                        }
                        if reader_proxy
                            .send_event(AppEvent::PtyOutput(buf[..n].to_vec()))
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(_) => break,
                }
            }

            match child.wait() {
                Ok(status) => diagnostics::record(&format!("PTY child status: {status:?}")),
                Err(err) => diagnostics::record(&format!("PTY child wait error: {err:#}")),
            }
            let _ = reader_proxy.send_event(AppEvent::PtyExited);
        });

        Ok(Self {
            master: pair.master,
            writer,
        })
    }

    pub fn write(&self, bytes: &[u8]) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    pub fn resize(&self, cols: u16, rows: u16, pixel_width: u16, pixel_height: u16) {
        let _ = self.master.resize(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            pixel_width,
            pixel_height,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_primary_device_query_across_pty_reads() {
        let mut query = PrimaryDeviceQuery::default();
        assert_eq!(query.observe(b"prompt\x1b[0"), 0);
        assert_eq!(query.observe(b"cother\x1b[c"), 2);
        assert_eq!(query.observe(b"\x1b[31m\x1b[1c"), 0);
        assert_eq!(query.observe(b"\x1b[0c"), 1);
    }

    #[test]
    fn starts_account_shell_without_shell_environment() {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open PTY");
        let mut settings = Settings::default();
        settings.use_fish = false;
        let mut command = shell_command(&settings);
        command.env_remove("SHELL");
        let mut child = pair
            .slave
            .spawn_command(command)
            .expect("start account shell");
        child.kill().expect("stop test shell");
        child.wait().expect("reap test shell");
    }

    #[test]
    fn fish_init_is_optional_and_quotes_installed_starship_path() {
        let mut settings = Settings::default();
        let init = fish_init(&settings, Some(Path::new("/tmp/star's ship")));
        assert!(init.contains("function fish_greeting; end"));
        assert!(init.contains("'/tmp/star\\'s ship' init fish | source"));
        assert!(init.contains("functions -q __starship_set_job_count"));
        assert_eq!(fish_init(&settings, None), "function fish_greeting; end");

        settings.show_fish_greeting = true;
        settings.use_starship = false;
        assert!(fish_init(&settings, Some(Path::new("/usr/bin/starship"))).is_empty());
    }
}
