# termchat

A fast, asynchronous, bi-directional chat server and client built entirely for the terminal.

<img src="./image.png" alt="termchat in action" />

## Features

- **Pure CLI Experience:** No heavy TUI frameworks (too lazy). Native standard I/O with clean padding and ANSI color formatting.
- **Asynchronous Networking:** Powered by `tokio` and `tokio-util` for instant, non-blocking message broadcasting over raw TCP streams.
- **Smart Routing:** The server tracks active connections in real-time, allowing for room rosters and system alerts.
- **Profile Management:** Persists your username locally via config so you don't have to type it on every connection.

## list of features

- [x] server and client connection
- [x] inline chat commands (/theme, /random shi)
- [x] inline chat completions
- [x] theme
- [x] /ask command for chatting with LLMs
- [ ] persistant chat or history
- [ ] better managment with user (auth and config idk)

## Installation

No Rust required — just grab the pre-built binary for your platform.

### Linux & macOS (one-liner)

```bash
curl -sSfL https://raw.githubusercontent.com/LHagfoss/termchat/main/scripts/install.sh | bash
```

This downloads the latest release binary and places it in `~/.local/bin`. Make sure that directory is in your `$PATH` (add `export PATH="$HOME/.local/bin:$PATH"` to your shell config if needed).

### Windows

**Option 1 — winget** (requires a release to be published first):
```powershell
winget install LHagfoss.termchat
```

**Option 2 — PowerShell one-liner:**
```powershell
irm https://raw.githubusercontent.com/LHagfoss/termchat/main/scripts/install.ps1 | iex
```

Or download the `.exe` directly from [Releases](https://github.com/LHagfoss/termchat/releases).

### Arch Linux (AUR)

```bash
yay -S termchat    # or your preferred AUR helper
```

You'll need to push a `PKGBUILD` to a personal AUR repo and update the version with each release. See `AUR/PKGBUILD` in this repo for the template.

### Homebrew (macOS / Linux)

Coming soon — will publish to a tap once we have stable releases.

### From source

If you do have Rust installed:

```bash
cargo install --path .
```

---

## Usage

```bash
# Start a server
termchat start

# Join a server
termchat join
```

See `termchat --help` for all commands and options.
