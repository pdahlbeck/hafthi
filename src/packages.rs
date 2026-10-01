use std::{fs, process::Command};

use anyhow::{bail, Context, Result};
use portable_pty::CommandBuilder;

use crate::pty;

pub fn arch_based() -> bool {
    fs::read_to_string("/etc/os-release")
        .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
        .is_ok_and(|text| arch_os_release(&text))
}

fn arch_os_release(text: &str) -> bool {
    text.lines().filter_map(|line| line.split_once('='))
        .filter(|(key, _)| matches!(*key, "ID" | "ID_LIKE"))
        .any(|(_, value)| value.trim().trim_matches(|c| c == '"' || c == '\'')
            .split_whitespace().any(|id| matches!(id, "arch" | "archlinux")))
}

pub fn yay_install_available() -> bool {
    arch_based() && pty::installed_program("pacman").is_some()
        && pty::installed_program("bash").is_some()
}

pub fn yay_install_command() -> Result<CommandBuilder> {
    if !yay_install_available() {
        bail!("Yay installation requires an Arch-based system with pacman and Bash");
    }
    let mut command = CommandBuilder::new(pty::installed_program("bash").context("Bash not found")?);
    command.args(["-c", include_str!("../scripts/install-yay.sh")]);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    Ok(command)
}

pub fn open_yay_installer() -> Result<()> {
    let _ = yay_install_command()?;
    Command::new(std::env::current_exe()?).arg("--install-yay").spawn()
        .context("Could not open the Yay installation window")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_arch_and_derivatives_without_matching_unrelated_names() {
        assert!(arch_os_release("ID=arch\n"));
        assert!(arch_os_release("ID=manjaro\nID_LIKE=\"arch\"\n"));
        assert!(arch_os_release("ID=endeavouros\nID_LIKE='archlinux arch'\n"));
        assert!(!arch_os_release("ID=ubuntu\nID_LIKE=debian\n"));
        assert!(!arch_os_release("NAME=Arch Linux\nID=notarch\n"));
        assert!(!arch_os_release("# ID=arch\n"));
    }
}
