<p align="center">
<img width="271" height="233" alt="image" src="https://github.com/user-attachments/assets/29604f3f-aa2e-405e-9a59-39cf955f593f" />
</p>
<h1 align="center"><code>herdr-glance</code></h1>

<p align="center">
  <img alt="Platform" src="https://img.shields.io/badge/platform-macOS-blue?style=flat-square" />
  <img alt="Herdr plugin" src="https://img.shields.io/badge/Herdr-plugin-56b4a9?style=flat-square" />
  <img alt="License" src="https://img.shields.io/github/license/arvmaan/herdr-glance?style=flat-square" />
  <img alt="Built with Rust and Tauri" src="https://img.shields.io/badge/built%20with-Rust%20%2F%20Tauri-orange?style=flat-square" />
</p>

<h3 align="center">Keep your agents in sight.</h3>

Herdr Glance is a tiny, always-on-top desktop widget for live
[Herdr](https://herdr.dev) agent status. It stays visible across macOS
workspaces, shows which agents are working or waiting, and lets you jump to an
agent without returning to the terminal first.

- **Three density modes** - A compact priority agent, a concise agent list, or
  full workspace and status detail.
- **Local or remote Herdr** - Run the Herdr CLI directly or reach it through
  non-interactive SSH.
- **Content-sized** - The window grows only as far as the current view needs.
- **Native widget behavior** - Always on top, visible across workspaces,
  draggable, and position-aware.
- **Direct Ghostty attach** - Click a pill or row to open that agent in
  Ghostty, locally or through SSH.
- **Light or dark** - A compact appearance toggle follows you across launches.
- **No frontend toolchain** - The Tauri shell serves plain HTML, CSS, and
  JavaScript.

| Platform | Status | Install |
|----------|--------|---------|
| macOS 13+ | Primary target | Install as a Herdr plugin or build a local `.app` |
| Windows | Not yet validated | Source may compile, but no packaged workflow exists |
| Linux | Core tests only | Desktop build requires the normal Tauri system libraries |

There is no notarized release yet. Build the app locally.

## Contents

- [Quick start](#quick-start)
- [Herdr plugin](#herdr-plugin)
- [Views](#views)
- [How it works](#how-it-works)
- [Connections](#connections)
- [Configuration](#configuration)
- [Development](#development)
- [Repository layout](#repository-layout)
- [Current limitations](#current-limitations)

## Quick start

Prerequisites:

- [Rust](https://rustup.rs)
- Xcode Command Line Tools
- [Ghostty](https://ghostty.org)
- Herdr 0.8.0 or newer

```sh
xcode-select --install
herdr plugin install arvmaan/herdr-glance
herdr plugin action invoke herdr.glance.open
```

Herdr builds Glance from source during installation. The first invocation opens
Connection settings with **Local** selected and uses the exact Herdr binary that
launched the plugin.

For a standalone application bundle, install Tauri CLI v2 and build locally:

```sh
cargo install tauri-cli --version "^2"

git clone https://github.com/arvmaan/herdr-glance.git
cd herdr-glance
./scripts/bundle-macos.sh --install
```

The script generates the application icons, builds a release bundle, applies
an ad-hoc signature, installs `Herdr Glance.app` in `/Applications`, and launches
it.

On first launch:

1. Choose **Local** or **SSH**.
2. Enter the Herdr executable name or absolute path.
3. For SSH, also enter the host or alias from your SSH configuration.
4. Select **Test**, then **Save**.

The compact widget opens after the connection is saved. Click its agent pill,
or any agent row in the larger views, to open a Ghostty window attached
directly to that agent.

## Herdr plugin

The root [`herdr-plugin.toml`](herdr-plugin.toml) exposes one action:

```text
herdr.glance.open
```

The plugin build compiles `herdr-glance-app` in release mode. The action starts
the widget as a detached native process, so the Herdr command returns while
Glance stays open.

Useful commands:

```sh
herdr plugin action invoke herdr.glance.open
herdr plugin list
herdr plugin log list --plugin herdr.glance
herdr plugin uninstall herdr.glance
```

The repository is discoverable through the
[Herdr marketplace](https://herdr.dev/plugins/) when its public GitHub
repository carries the `herdr-plugin` topic.

## Views

Use the view button beside the settings control to cycle through three modes:

| Mode | Shows | Typical size |
|------|-------|--------------|
| **Compact** | Priority agent name, status color, and additional-agent count | `200 x 36` |
| **List** | Agent names and color-coded state | Width `260`, height follows content |
| **Detail** | Agent name, workspace/tab, state, Active/All filter, and last update | Width `360`, height follows content |

The widget caps the visible rows and scrolls longer lists. Its position is
restored across launches, while the selected density mode is stored in the
webview's local storage. Open Connection settings to switch between light and
dark appearance.

Status colors are consistent in every view:

| Status | Meaning |
|--------|---------|
| `working` | The agent is actively running |
| `blocked` | The agent needs attention |
| `idle` | The agent is waiting |
| `done` | The agent has completed |
| `unknown` | Herdr returned an unrecognized state |

## How it works

```text
                           every two seconds
                                  |
                                  v
Herdr Glance UI  <--- Tauri IPC --- Rust core
                                      |
                  +-------------------+-------------------+
                  |                                       |
                  v                                       v
       <herdr> api snapshot                 ssh <host> <herdr> api snapshot
       <herdr> agent attach <id>            ssh -t <host> <herdr> agent attach <id>
                  |                                       |
                  +-------------------+-------------------+
                                      |
                                      v
                              Herdr socket API
```

The Rust core validates the connection, runs Herdr locally or over SSH, parses
the snapshot JSON into a stable agent model, and returns it to the frontend.
The UI polls every two seconds. Clicking an agent opens Ghostty with a direct
`herdr agent attach` session. SSH connections run the attachment on the remote
host, so Herdr does not need to be installed on the laptop. Failed polls clear
stale rows and turn the connection indicator red.

SSH commands use:

```text
BatchMode=yes
ConnectTimeout=5
```

This prevents password prompts from hanging the widget. The interactive
Ghostty attachment can still show an SSH password or key prompt. Remote
executable paths and command arguments are shell-quoted before execution.

## Connections

### Local

Choose **Local** when Herdr runs on the same machine as the widget. The
executable may be on the GUI application's `PATH`, or you can provide an
absolute path:

```text
/Users/you/.local/bin/herdr
```

macOS apps launched from Finder often receive a smaller `PATH` than your shell,
so an absolute path is the most reliable option.

### SSH

Choose **SSH** when Herdr runs on another machine. Public-key authentication
must already work without an interactive prompt.

Verify the connection in a terminal first:

```sh
ssh herdr-host /home/you/.local/bin/herdr api snapshot
```

Then use:

```text
SSH host:        herdr-host
Herdr executable: /home/you/.local/bin/herdr
```

SSH aliases, proxy settings, identities, and ports belong in `~/.ssh/config`.
Herdr Glance passes the configured host to the system `ssh` command.

## Configuration

Connection settings are stored at:

```text
~/.config/herdr-glance/config.json
```

Local example:

```json
{
  "ssh_target": "",
  "remote_herdr": "/Users/you/.local/bin/herdr"
}
```

SSH example:

```json
{
  "ssh_target": "herdr-host",
  "remote_herdr": "/home/you/.local/bin/herdr"
}
```

An empty `ssh_target` selects local execution. Existing SSH configurations from
the previous app name are loaded automatically; the next save writes them under
the Glance config directory. Appearance and density preferences remain local to
the webview.

## Development

Run the core tests without desktop GUI dependencies:

```sh
cargo test -p herdr-glance-core
cargo clippy -p herdr-glance-core --all-targets -- -D warnings
```

Run the desktop app on macOS:

```sh
cd crates/herdr-glance-app
cargo tauri dev
```

Build the release app:

```sh
./scripts/bundle-macos.sh
```

Additional static checks:

```sh
cargo fmt --all --check
node --check ui/app.js
jq empty crates/herdr-glance-app/tauri.conf.json
bash -n scripts/bundle-macos.sh
```

## Repository layout

```text
crates/
  herdr-glance-core/
    src/
      config.rs        # connection validation and persistence
      herdr.rs         # local/SSH execution and snapshot parsing
  herdr-glance-app/
    src/
      commands.rs      # Tauri IPC commands and window sizing
      main.rs          # app setup and always-on-top behavior
    capabilities/      # Tauri window permissions
    tauri.conf.json    # native window and bundle configuration
ui/
  index.html           # widget and connection settings
  style.css            # compact/list/detail layouts
  app.js               # polling, rendering, and view state
assets/
  icon.png             # source application icon
herdr-plugin.toml      # Herdr marketplace manifest
scripts/
  bundle-macos.sh      # build, sign, install, and launch
  open-plugin.sh       # detached Herdr action launcher
```

The frontend has no Node dependency or build step.

## Current limitations

- macOS is the only validated desktop target.
- The app is ad-hoc signed and not notarized.
- Connection polling and testing require key-based, non-interactive SSH
  authentication.
- Agent clicks require Ghostty to be installed.
- macOS runtime behavior still needs validation on a physical Mac after each
  window-shell change.

## License

MIT
