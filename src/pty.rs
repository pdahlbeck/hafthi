use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    thread,
};

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use winit::event_loop::EventLoopProxy;

#[derive(Debug)]
pub enum AppEvent {
    PtyOutput(Vec<u8>),
    PtyExited,
    ImageChosen(Option<String>),
}

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

fn shell_command() -> CommandBuilder {
    // A graphical launcher may omit SHELL. portable-pty resolves the login
    // shell from the account database when it is absent.
    let mut cmd = CommandBuilder::new_default_prog();
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "Hafthi");
    cmd.env("HAFTHI", "1");
    cmd
}

impl PtySession {
    pub fn spawn(cols: u16, rows: u16, proxy: EventLoopProxy<AppEvent>) -> Result<Self> {
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
            .spawn_command(shell_command())
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
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
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

            let _ = child.wait();
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
    fn starts_account_shell_without_shell_environment() {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open PTY");
        let mut command = shell_command();
        command.env_remove("SHELL");
        let mut child = pair
            .slave
            .spawn_command(command)
            .expect("start account shell");
        child.kill().expect("stop test shell");
        child.wait().expect("reap test shell");
    }
}
