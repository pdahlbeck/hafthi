"""Exercise the installer using fake package tools; never run real sudo/pacman."""
import os
from pathlib import Path
import subprocess
import tempfile

source = Path(__file__).resolve().parents[1] / "scripts/install-yay.sh"
subprocess.run(["bash", "-n", str(source)], check=True)

def run_case(name, distro="arch", repo=False, installed=False, fail=False, root=False, channel="stable"):
    with tempfile.TemporaryDirectory() as temp:
        folder = Path(temp)
        tools = folder / "bin"
        tools.mkdir()
        log = folder / "commands"
        release = folder / "os-release"
        release.write_text(f"ID={distro}\n")
        script = folder / "install.sh"
        # Only the fixture copy reads fake system metadata.
        script.write_text(source.read_text().replace("/etc/os-release", str(release)))
        def tool(name, body):
            path = tools / name
            path.write_text("#!/bin/sh\nset -eu\n" + body + "\n")
            path.chmod(0o755)
        tool("id", f"echo {0 if root else 1000}")
        tool("pacman", f"exit {0 if repo else 1}")
        tool("sudo", 'echo "sudo $*" >> "$TEST_LOG"\n' + ("exit 42" if fail else
            'case "$*" in *"base-devel yay") touch "$TEST_INSTALLED";; esac'))
        tool("git", 'echo "git $*" >> "$TEST_LOG"\nmkdir -p "$4"')
        tool("makepkg", 'echo "makepkg $*" >> "$TEST_LOG"\ntouch "$TEST_INSTALLED"')
        yay_body = 'echo "yay $*" >> "$TEST_LOG"\necho "yay test"'
        if installed:
            tool("yay", yay_body)
        else:
            # Install mocks add a Yay executable to a second PATH directory.
            destination = folder / "installed"
            destination.mkdir()
            # A wrapper for touch makes the mock installation create the command.
            tool("touch", 'printf "#!/bin/sh\\necho yay-test\\n" > "$TEST_INSTALLED"\nchmod +x "$TEST_INSTALLED"')
        env = dict(os.environ, PATH=f"{tools}:{folder / 'installed'}:/usr/bin:/bin",
                   TMPDIR=str(folder), TEST_LOG=str(log), TEST_INSTALLED=str(folder / "installed/yay"))
        result = subprocess.run(["bash", str(script), channel], input="\n", text=True,
                                capture_output=True, env=env)
        calls = log.read_text() if log.exists() else ""
        if distro != "arch" or root or channel not in ("stable", "development"):
            assert result.returncode != 0 and not calls, (name, result, calls)
        elif installed:
            assert result.returncode == 0 and "sudo" not in calls, (name, result, calls)
        elif fail:
            assert result.returncode == 42 and "git clone" not in calls and "makepkg" not in calls, (name, result, calls)
        elif repo and channel == "stable":
            assert result.returncode == 0 and "base-devel yay" in calls and "git clone" not in calls, (name, result, calls)
        else:
            package = "yay-git" if channel == "development" else "yay"
            assert result.returncode == 0 and f"https://aur.archlinux.org/{package}.git" in calls and "makepkg -si" in calls, (name, result, calls)
            assert "base-devel yay" not in calls, (name, result, calls)
        assert not list(Path(temp).glob("hafthi-yay.*"))
        print(f"PASS: {name}")

run_case("official AUR build")
run_case("distribution package", repo=True)
run_case("already installed", installed=True)
run_case("cancelled dependencies", fail=True)
run_case("unsupported system", distro="ubuntu")
run_case("refuse root", root=True)

run_case("development uses yay-git even with a repo package", repo=True, channel="development")
run_case("development cancelled dependencies", fail=True, channel="development")
run_case("reject unknown version", channel="invalid")
