use std::{fs, process::Command};

use anyhow::{bail, Context, Result};
use portable_pty::CommandBuilder;

use crate::pty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum YayVersion {
    #[default]
    Stable,
    Development,
}

impl YayVersion {
    pub fn from_arg(value: Option<&str>) -> Result<Self> {
        match value {
            None | Some("stable") => Ok(Self::Stable),
            Some("development") => Ok(Self::Development),
            _ => bail!("Unknown Yay version; choose stable or development"),
        }
    }

    pub fn as_arg(self) -> &'static str {
        match self { Self::Stable => "stable", Self::Development => "development" }
    }
}

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

pub fn yay_install_command(version: YayVersion) -> Result<CommandBuilder> {
    if !yay_install_available() {
        bail!("Yay installation requires an Arch-based system with pacman and Bash");
    }
    let mut command = CommandBuilder::new(pty::installed_program("bash").context("Bash not found")?);
    command.args(["-c", include_str!("../scripts/install-yay.sh"), "hafthi-yay-installer", version.as_arg()]);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    Ok(command)
}

pub fn open_yay_installer(version: YayVersion) -> Result<()> {
    let _ = yay_install_command(version)?;
    Command::new(std::env::current_exe()?).args(["--install-yay", version.as_arg()]).spawn()
        .context("Could not open the Yay installation window")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_supported_yay_versions() {
        assert_eq!(YayVersion::from_arg(None).unwrap(), YayVersion::Stable);
        for version in [YayVersion::Stable, YayVersion::Development] {
            assert_eq!(YayVersion::from_arg(Some(version.as_arg())).unwrap(), version);
        }
        assert!(YayVersion::from_arg(Some("unknown")).is_err());
    }

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
