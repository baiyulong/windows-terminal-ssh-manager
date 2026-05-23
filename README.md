# 🖥️ Windows Terminal SSH Manager

A standalone Windows binary that keeps your SSH server configurations in one
place, encrypts credentials with **Windows DPAPI**, and injects a profile for
every server directly into **Windows Terminal** via
[JSON Fragment Extensions](https://learn.microsoft.com/en-us/windows/terminal/json-fragment-extensions).

> **No Python, no Node, no OpenSSL.** One self-contained `ssh-manager.exe`.

![Build](https://github.com/baiyulong/windows-terminal-ssh-manager/actions/workflows/build.yml/badge.svg?branch=rust)

---

## ✨ Features

| Feature | Detail |
|---------|--------|
| **Encrypted storage** | Passwords encrypted with Windows DPAPI — only your OS user can decrypt them |
| **Windows Terminal integration** | One profile per server, auto-injected via Fragment Extensions |
| **Interactive picker** | `ssh-manager connect` shows a fuzzy-search list when no name is given |
| **Full PTY session** | Terminal resize, colour, arrow keys, Ctrl shortcuts all work |
| **Mouse forwarding** | VT mouse tracking (used by tools like GitHub Copilot CLI) forwarded correctly |
| **Zero dependencies** | Pure-Rust SSH2 via `russh` — no native C libraries required |

---

## 📋 Requirements

| Requirement | Notes |
|-------------|-------|
| Windows 10 / 11 | DPAPI is Windows-only |
| [Windows Terminal](https://aka.ms/terminal) | For Fragment profile injection |
| [Rust toolchain](https://rustup.rs) | Only needed to **build from source** |

---

## 🚀 Installation

### Option A — Download pre-built binary (recommended)

1. Go to the [Releases](https://github.com/baiyulong/windows-terminal-ssh-manager/releases) page.
2. Download `ssh-manager.exe` from the latest release.
3. Move it somewhere on your `PATH`, e.g.:
   ```
   %LOCALAPPDATA%\Microsoft\WindowsApps\
   ```
4. Open a new terminal and run:
   ```
   ssh-manager sync
   ```
   This writes the Windows Terminal Fragment file.  
   **Restart Windows Terminal** once so it picks up the new profiles.

### Option B — Build from source

```bat
git clone https://github.com/baiyulong/windows-terminal-ssh-manager.git
cd windows-terminal-ssh-manager
git checkout rust
install.bat
```

`install.bat` runs `cargo build --release`, copies the binary to
`%LOCALAPPDATA%\Microsoft\WindowsApps\`, and calls `ssh-manager sync`
automatically.

> **First build** downloads crate dependencies and may take ~2 minutes.
> Subsequent builds are incremental (seconds).

---

## 📖 Usage

```
ssh-manager <COMMAND>
```

| Command | Description |
|---------|-------------|
| `add` | Interactive wizard to add a new server |
| `list` | Show all configured servers |
| `connect [NAME]` | Connect via SSH; omit NAME for fuzzy picker |
| `test <NAME>` | Test connection and measure latency |
| `edit <NAME>` | Edit host / port / username / password / description |
| `remove <NAME>` | Delete a server (asks for confirmation) |
| `sync` | Regenerate the Windows Terminal Fragment file |

### Quick start

```powershell
# Add your first server
ssh-manager add

# List servers
ssh-manager list

# Connect (interactive picker)
ssh-manager connect

# Connect directly
ssh-manager connect prod-web

# Test a connection
ssh-manager test prod-web
```

### Inside an SSH session

| Action | How |
|--------|-----|
| Exit session | Type `exit` or press **Ctrl+D** |
| Copy text (normal) | Left-click drag to select → right-click → Copy |
| Copy text (mouse-tracking apps, e.g. Copilot) | **Shift+drag** to select → right-click → Copy |
| Paste | Right-click → Paste (Windows Terminal default) |

---

## 🗂️ How it works

```
~/.wt-ssh-manager/
└── config.json          ← encrypted server list (DPAPI base64)

%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\
└── wt-ssh-manager\
    └── profiles.json    ← Fragment file Windows Terminal reads on startup
```

1. **DPAPI encryption** — `CryptProtectData` / `CryptUnprotectData` from
   `Crypt32.dll` are called directly via raw FFI.  No master password is
   needed; the blob is bound to your Windows user account.

2. **Fragment profiles** — Each server gets a UUID5 profile GUID (derived
   from the server name, matching the Windows Terminal Fragment spec).  The
   profile `commandline` is `ssh-manager.exe connect <server-id>`.

3. **Interactive PTY** — `russh` opens an SSH channel, requests a
   `xterm-256color` PTY, and starts a shell.  `crossterm` reads keyboard and
   resize events from the local terminal and forwards them over the channel.
   VT mouse-tracking sequences are detected in the server output and mouse
   events are forwarded automatically.

---

## 🗑️ Uninstall

```powershell
# 1. Remove the binary
Remove-Item "$env:LOCALAPPDATA\Microsoft\WindowsApps\ssh-manager.exe"

# 2. Remove Windows Terminal Fragment profiles
Remove-Item -Recurse "$env:LOCALAPPDATA\Microsoft\Windows Terminal\Fragments\wt-ssh-manager"

# 3. Remove encrypted config (optional — contains your server list)
Remove-Item -Recurse "$env:USERPROFILE\.wt-ssh-manager"
```

Restart Windows Terminal after step 2 so the injected profiles disappear.

---

## 🔒 Security notes

- Passwords are encrypted with **Windows DPAPI** and are only recoverable by
  the same Windows user account on the same machine.
- The config file (`~/.wt-ssh-manager/config.json`) contains DPAPI-encrypted
  blobs — copying it to another machine or user account renders the passwords
  unreadable.
- SSH host keys are **not verified** (equivalent to `StrictHostKeyChecking=no`).
  Suitable for trusted internal networks; not recommended for untrusted
  internet hosts without adding host-key pinning.

---

## 🛠️ CI / Build pipeline

Every push to the `rust` branch and every pull request triggers a
**GitHub Actions** workflow (`.github/workflows/build.yml`) that:

1. Builds a release binary on `windows-latest`
2. Runs a smoke test (`--version`, `--help`)
3. Uploads `ssh-manager.exe` as a build artifact (retained 30 days)

When you push a **semver tag** (e.g. `v1.0.0`), an additional job creates a
**GitHub Release** and attaches the binary automatically:

```powershell
git tag v1.0.0
git push origin v1.0.0
```

---

## 🤝 Contributing

```bat
git clone https://github.com/baiyulong/windows-terminal-ssh-manager.git
cd windows-terminal-ssh-manager
git checkout rust
cargo build          # debug build
cargo test           # run tests
cargo clippy         # lint
```

Pull requests welcome!
