# wsl-gpg-agent

`wsl-gpg-agent` allows your WSL (Windows Subsystem for Linux)  Linux environments to communicate with your Windows environment's GPG installation for YubiKey, GPG, and SSH support.

## Installation

### Windows Environment

#### Dependencies

Please follow the following guide from Yubico to install and configure GPG: https://developers.yubico.com/PGP/SSH_authentication/Windows.html.

### Linux Environment

#### wsl-gpg-agent binary

```bash
mkdir -p $HOME/.local/bin # make the .local/bin directory if it doesn't already exist
wget -O "$HOME/.local/bin/wsl-gpg-agent.exe" "https://github.com/tmuntaner/wsl-gpg-agent/releases/latest/download/wsl-gpg-agent.exe"
chmod +x "$HOME/.local/bin/wsl-gpg-agent.exe"
```

#### Dependencies

To use this agent, you'll need to have `socat` and `ss` installed in your system.

##### openSUSE

```bash
sudo zypper in socat iproute2
```

#### Shell Configuration

Please add the following to your shell configuration file (`~/.zshrc`, `~/.bashrc`) to set up your GPG and SSH sockets.

##### Bash/ZSH

```bash
export SSH_AUTH_SOCK="$HOME/.ssh/agent.sock"
if ! ss -a | grep -q "$SSH_AUTH_SOCK"; then
  rm -f "$SSH_AUTH_SOCK"
  wsl_gpg_agent_bin="$HOME/.local/bin/wsl-gpg-agent.exe"
  if test -x "$wsl_gpg_agent_bin"; then
    (setsid nohup socat UNIX-LISTEN:"$SSH_AUTH_SOCK,fork" EXEC:"$wsl_gpg_agent_bin ssh" > /dev/null 2>&1 &)
  else
    echo >&2 "WARNING: $wsl2_ssh_pageant_bin is not executable."
  fi
  unset wsl2_ssh_pageant_bin
fi

export GPG_AGENT_SOCK="$HOME/.gnupg/S.gpg-agent"
if ! ss -a | grep -q "$GPG_AGENT_SOCK"; then
  rm -rf "$GPG_AGENT_SOCK"
  wsl_gpg_agent_bin="$HOME/.local/bin/wsl-gpg-agent.exe"
  if test -x "$wsl_gpg_agent_bin"; then
    (setsid nohup socat UNIX-LISTEN:"$GPG_AGENT_SOCK,fork" EXEC:"$wsl_gpg_agent_bin gpg" > /dev/null 2>&1 &)
  else
    echo >&2 "WARNING: $wsl_gpg_agent_bin is not executable."
  fi
  unset wsl2_ssh_pageant_bin
fi
```