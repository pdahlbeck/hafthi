use std::{fs, path::PathBuf, process::Command};

use anyhow::{bail, Context, Result};
use portable_pty::CommandBuilder;

use crate::{pty, settings::config_path};

pub fn sampler_config_path() -> PathBuf {
    config_path().with_file_name("sampler.yml")
}

pub fn ensure_sampler_config() -> Result<PathBuf> {
    let path = sampler_config_path();
    if !path.exists() {
        fs::create_dir_all(path.parent().context("invalid Sampler config directory")?)?;
        // create_new leaves an existing user config untouched, including when
        // two Hafþi windows open the dashboard at the same time.
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(include_bytes!("../assets/sampler-default.yml"))?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}

pub fn command(name: &str) -> Result<CommandBuilder> {
    let executable = match name {
        "sampler" | "yazi" | "micro" => pty::installed_program(name)
            .with_context(|| format!("{name} is not installed"))?,
        _ => bail!("unknown plugin: {name}"),
    };
    let mut cmd = CommandBuilder::new(executable);
    if let Some(home) = std::env::var_os("HOME") {
        cmd.cwd(PathBuf::from(home));
    }
    if name == "sampler" {
        cmd.arg("-c");
        cmd.arg(ensure_sampler_config()?);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "Hafthi");
    cmd.env("HAFTHI", "1");
    Ok(cmd)
}

pub fn open_window(name: &str) -> Result<()> {
    // Check before opening a second app window, so missing packages do not
    // leave an empty terminal. The child checks again before starting its PTY.
    let _ = command(name)?;
    Command::new(std::env::current_exe()?)
        .args(["--plugin", name])
        .spawn()
        .with_context(|| format!("could not open {name} window"))?;
    Ok(())
}
