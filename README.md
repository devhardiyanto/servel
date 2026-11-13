# Servel

> **One app. All your local dev environment.**

Servel is a desktop app (Tauri 2 + Vue 3) that runs your local development environment from one window:

- **Services**: MySQL, PostgreSQL, Redis and friends in containers (WSL containers on Windows, or Docker).
- **Projects**: your Laravel, Vite, Nuxt, Node or Go apps, started for you on a fixed `localhost:<port>`.
- **Versions**: PHP and Node per project through [mise](https://mise.jdx.dev), plus PHP extensions per version.

No more juggling three terminals every morning.

![Dashboard](docs/screenshots/dashboard-running.png)

---

## Features

**Projects**
- Add a project folder; Servel detects what it is (Laravel, Lumen, WordPress, Drupal, plain PHP, Vite with Vue/React/Svelte, Next, Nuxt, Angular, other Node apps, Go) and suggests the dev-server command and a free port.
- Start, Stop, Restart and Open each project at `localhost:<port>`; its output goes to the Logs viewer.
- The dev server runs with the PHP/Node/Go versions from the project's `mise.toml`, plus any environment variables you set for that project.
- Optionally start a project when Servel opens, and link it to a service profile.
- Dev servers stop when Servel quits, and do not linger if Servel crashes.

**Services**
- MySQL, PostgreSQL, Redis, RabbitMQ, MongoDB, MinIO, Mailpit, Gotenberg. Toggle what you need, click Start.
- Windows: runs on WSL containers (`wslc`, built into WSL 3.0.1+). No Docker Desktop and no extra Linux distro needed. Docker stays available.
- A service shows "up" only once it accepts connections.
- Service profiles, database restore from a `.sql` dump, and import from an old Docker setup.

**PHP and Node**
- With mise: see and switch PHP/Node versions per project, written to the `mise.toml` that actually applies. phpvm and fnm are still supported.
- PHP extensions: turn bundled extensions on or off per PHP version. `php.ini` is backed up first and the change is undone if PHP no longer starts.
- Windows: one click fixes HTTPS from PHP (curl error 60).

**App**
- In-app updates from Settings → About.
- System tray, remembered selection, and a `servel` terminal command on Windows.

---

## Install

Download the installer for your OS from the [Releases page](https://github.com/devhardiyanto/servel/releases). File names below use `<version>` (for example `2.3.0`).

> Servel ships **unsigned** for Windows and macOS. Workarounds are documented below; see the FAQ for the long-term plan.

### Windows

1. Download `Servel_<version>_x64-setup.exe` from Releases.
2. Run the installer. Windows SmartScreen will warn: *"Windows protected your PC"*. Click **More info** → **Run anyway**.
3. When prompted about `.wslconfig`, choose:
   - **Yes (Overwrite)**: replace your existing `%USERPROFILE%\.wslconfig` with the Servel-recommended one (the old file is backed up to `.wslconfig.bak`).
   - **No (Append)**: append Servel's WSL2 settings to the bottom of your existing file.
   - **Cancel (Skip)**: leave `.wslconfig` untouched. Recommended if you use WSL containers and already tuned WSL yourself.
4. The installer also adds the `servel` terminal command to your user PATH (see [`servel` terminal command](#servel-terminal-command-windows)).
5. Launch Servel from the Start Menu. With WSL 3.0.1+, Servel uses the built-in WSL containers automatically.

**Updating:** from v2.1 on, open **Settings → About → Check for updates → Update now**. Servel stops running projects, installs the new version and reopens. Older versions update by installing the new release manually once.

If you changed `.wslconfig`, restart WSL once so it takes effect:

```powershell
wsl --shutdown
```

### macOS

1. Download `Servel_<version>_aarch64.dmg` (Apple Silicon) or `Servel_<version>_x64.dmg` (Intel) from Releases.
2. Open the DMG, drag **Servel.app** into your **Applications** folder.
3. On first launch macOS Gatekeeper blocks the app because it is unsigned. Open Terminal and run:
   ```bash
   xattr -d com.apple.quarantine /Applications/Servel.app
   ```
4. Launch Servel from Applications or Spotlight.

### Linux

1. Download `Servel_<version>_amd64.AppImage` from Releases.
2. Make it executable and run:
   ```bash
   chmod +x Servel_<version>_amd64.AppImage
   ./Servel_<version>_amd64.AppImage
   ```

---

## Prerequisites

Servel orchestrates external tools — it does not install them for you. Make sure these are available on your PATH before first run.

### Container engine

#### Windows — WSL containers (recommended)

Since v1.6 Servel can run services on **WSL containers** (`wslc.exe`), the container engine that ships with WSL 3.0.1 and newer. You do not need Docker Desktop or a dedicated Linux distro.

```powershell
wsl --update
wsl --version   # WSL version must be 3.0.1 or newer
```

Servel picks the engine automatically (**Settings → Container engine → Auto**). Since v2.0, WSL containers are the default on Windows; Servel stays on Docker only when `wslc` is missing or Docker still holds your `servel_*` containers. You can force either one in Settings.

Notes:

- WSL containers keep their **own images and volumes**, separate from Docker Desktop. Data in Docker volumes does not move over automatically; restore your database dumps after switching.
- Run Servel **without** "Run as administrator". WSL containers keep one session per elevation, so containers created from an elevated window are invisible to a normal one, and the reverse.
- Ports are published on `127.0.0.1` only.
- Containers named `servel_<service>` that already exist (for example, created by a script) are **adopted as-is**: Servel starts and stops them but does not recreate them.

#### `servel` terminal command (Windows)

The Windows installer adds a `servel` command to your user PATH (`%LOCALAPPDATA%\Servel\cli\bin`). It works in PowerShell, cmd, and Git Bash / zsh, and manages the same services as the app through WSL containers:

```bash
servel preflight                 # check wslc, WSL version, elevation, ports
servel up                        # start the services selected in the app (or all)
servel up mysql postgres         # only some
servel down                      # stop every servel_* container
servel status
servel logs mysql -f
servel shell postgres            # shell inside the container
servel restore mysql D:\backup\dump.sql   # or a folder of .sql files
servel use php 8.3               # via mise, in the current project folder
servel use node 22 --global      # change the global version
servel projects                  # list projects from the app, with their localhost URLs
servel start share-api           # start a project's dev server (the Servel app must be open)
servel stop share-api
servel restart share-api
servel ps                        # state and URL of every project
```

The app and the CLI share state, so you can use them interchangeably. Open a **new** terminal after installing so it picks up the new PATH. If you previously aliased `servel` in your shell profile, remove that alias; otherwise it shadows the bundled command.

MSYS2 shells started with `MSYS2_PATH_TYPE=minimal` do not inherit the Windows PATH. Add this line to `~/.bashrc` / `~/.zshrc` instead:

```bash
export PATH="$PATH:$(cygpath "$LOCALAPPDATA")/Servel/cli/bin"
```

#### Docker

- **Windows / macOS:** [Docker Desktop](https://www.docker.com/products/docker-desktop/) — make sure it is running before starting any service from Servel.
- **Linux:** [Docker Engine](https://docs.docker.com/engine/install/) + the Compose v2 plugin. Add your user to the `docker` group so you do not need `sudo`.

### mise — PHP, Node and Go versions (recommended)

- Site: [mise.jdx.dev](https://mise.jdx.dev). Install it, then install the versions you need, for example `mise use -g php@8.3 node@22`.
- When mise is installed, Servel uses it automatically (**Settings → PHP & Node → Manage PHP & Node with → Auto**).
- Versions are resolved per folder, so a `mise.toml` in a project (or one of its parent folders) decides which PHP/Node that project uses, both in Servel and in your terminal.

### phpvm and fnm (alternative)

Without mise, Servel switches versions with [phpvm](https://github.com/devhardiyanto/phpvm) and [fnm](https://github.com/Schniz/fnm). Both are optional.

- phpvm: install from its repo, then add the hook to your shell profile with `phpvm hook`.
- fnm: `winget install Schniz.fnm`, `brew install fnm` or `cargo install fnm`, then add its shell hook as documented in the fnm README.

---

## First run

On first launch Servel checks what is installed: the container engine (WSL containers or Docker) and mise (or phpvm + fnm). Missing items show what to install. Click **Refresh checks** after installing, then **Continue to Servel**.

---

## Using Servel

### Services (Dashboard)

1. Toggle the services you need. The selection is saved as a **profile**; create more profiles in **Settings → Services**.
2. Click **Start selected**. Each service shows "starting" until it accepts connections, then "up".
3. **Stop all** stops every service; the data stays in volumes.

Services listen on `127.0.0.1` with these defaults (for local development only):

| Service | Port | Login |
|---|---|---|
| MySQL | 3306 | `root`, no password; database `local_db` |
| PostgreSQL | 5432 | `postgres`, no password; database `local_db` |
| Redis | 6379 | none |
| RabbitMQ | 5672, UI 15672 | `guest` / `guest` |
| MongoDB | 27017 | none |
| MinIO | 9000, console 9001 | `admin` / `password` |
| Mailpit | SMTP 1025, UI 8025 | none |
| Gotenberg | 3030 | none |

To load data: **Settings → Services → Restore database** (pick a `.sql` dump for MySQL or PostgreSQL). Coming from Docker on Windows: **Import from Docker** copies your databases into WSL containers.

### Projects (Settings → Projects)

1. Click **+ Add project folder** and pick the folder. Servel detects the type and opens the form with a suggested **Command** and **Port**.
2. Check the form, then **Save & start**.
3. When the status turns **running**, click `localhost:<port>` to open it.

| Field | What it does |
|---|---|
| **Command** | Any command, run in the project folder. `{port}` is replaced by the port, and the `PORT` variable is set to it. **Detect** fills it in again. Leave it empty to use the project only for versions and services. |
| **Port** | Fixed port on `localhost` (1024–65535). |
| **Services** | Service profile applied when you pick this project under **Versions for** on the Dashboard. Tick **Start these services when the project starts** to have Start, Restart and start-on-launch also start that profile's services; running services are left alone and stopping the project stops none. |
| **Environment** | One `KEY=VALUE` per line, set for this project's dev server on top of mise's environment. Lines copied from a shell (`export KEY='value'`) work as-is. |
| **On launch** | Start this project when Servel opens. |

Examples of commands Servel suggests:

| Project | Command |
|---|---|
| Laravel | `php artisan serve --host=127.0.0.1 --port={port}` |
| Lumen / plain PHP | `php -S 127.0.0.1:{port} -t public` |
| WordPress | `php -S 127.0.0.1:{port}` |
| Vite (Vue/React) | `npm run dev -- --port {port} --strictPort` |
| Next | `npm run dev -- -p {port}` |
| Node backend | `npm run dev` (reads `PORT`) |
| Go | `go run .` or `air` (reads `PORT`) |

`npm` becomes `pnpm`, `yarn` or `bun` when that lockfile is present. **Restart** stops the project, waits for the port to be free and starts it again. Output shows under **Logs → Projects**. There, pick a project chip to see only its output, or **Errors & warnings** to hide everything else; each project keeps its own last 500 lines.

Notes:

- Node backends and Go apps follow the port only if they read the `PORT` variable. Otherwise set the app's own port in **Port**; the status shows "port not answering" until something listens on it.
- Dependencies are not installed for you. Run `composer install` / `npm install` first; if a project exits, its card shows the last output line.
- On Windows, `php -S` and `artisan serve` handle one request at a time.

### PHP and Node versions

- **Dashboard → Versions for** picks the context: **Global** or one of your projects. The PHP and Node lists show the versions that apply there and where they come from (project `mise.toml` or global config).
- Switching a version in a project context writes to that project's `mise.toml`. If the version currently comes from your global config, Servel asks whether to pin it in the project or change it globally.
- A `mise.toml` that mise does not trust yet shows a **Trust** button.

### PHP extensions (Settings → PHP & Node)

1. Pick the PHP version at the top of **PHP extensions**.
2. Turn extensions on or off with their switches. The change goes into that version's `php.ini` and applies to every project and terminal that uses the version. Restart running PHP projects to pick it up.
3. **Windows:** if **HTTPS certificates** says "Not set", click **Fix HTTPS certificates**. This sets `curl.cainfo` and `openssl.cafile`, which fixes curl error 60.

Before each change Servel backs up `php.ini` next to it (`php.ini.servel-<time>.bak`, last 5 kept). If PHP fails to start afterwards, the old `php.ini` is restored and the reason is shown.

Extensions that are not bundled (redis, xdebug, imagick, mongodb, ...) can be downloaded on Windows: type the name under **Install another extension** and click **Download & enable**. Servel picks the build from downloads.php.net that matches your PHP version, thread safety and architecture, puts the DLL in the `ext` folder (support libraries, as imagick has, go next to `php.exe`) and turns the extension on. If PHP rejects it, the DLL is removed and `php.ini` is put back. The download site publishes no checksums, so the only integrity check is HTTPS to php.net. On macOS and Linux, install extensions with `pecl install`; they then appear in the list.

### Tray menu

Right-click the tray icon for **Show Servel**, the **PHP** and **Node** version submenus, **Start all selected**, **Stop all** and **Quit Servel**. Closing the window keeps Servel in the tray (change it in **Settings → General**). Quitting stops running projects; services keep running until you stop them.

---

## FAQ & Troubleshooting

### "Port already in use" when starting a service

Another process is bound to the port Servel needs. Find it and stop it:

```powershell
# Windows
netstat -ano | findstr :3306

# macOS / Linux
lsof -i :3306
```

Stop the offending process and click Start again. For a project, pick another **Port** in its Edit form.

### "WSL containers is not responding" (Windows)

1. Make sure WSL is up to date: `wsl --update`, then `wsl --version` (3.0.1 or newer).
2. Restart the WSL service: `wsl --shutdown`, then click **Retry** in Servel.
3. `ERROR_SHARING_VIOLATION` on every command means the WSL container session is locked. `wsl --shutdown` clears it; if it persists, run `Restart-Service WslService -Force` from an elevated PowerShell.
4. `Too many volumes have been mounted` means the per-session mount limit was hit. `wsl --shutdown` resets it.

### Databases are empty after switching to WSL containers

Expected: WSL containers and Docker Desktop have separate storage. Use **Settings → Services → Import from Docker** (Docker must be running), or **Restore database** with a `.sql` dump. You can also switch back to Docker in **Settings → Container engine**.

### "Docker not running" / containers fail to start

Start Docker Desktop (Windows/macOS) or the Docker daemon (Linux: `sudo systemctl start docker`) before clicking Start in Servel.

### `phpvm` hook not installed — PHP version does not change

Make sure your shell profile (`~/.bashrc`, `~/.zshrc`, or PowerShell `$PROFILE`) sources the phpvm hook. Run `phpvm hook` and follow the printed instructions, then restart your shell.

### `.wslconfig` settings not applied (Windows)

WSL only reads `.wslconfig` at startup. After install (or after editing the file), run:

```powershell
wsl --shutdown
```

Then start Servel (and Docker Desktop, if you use it) again.

### Windows SmartScreen blocks the installer

Servel releases are currently **unsigned**, so SmartScreen shows a warning. Click **More info** → **Run anyway**.

### macOS says "Servel is damaged and can't be opened"

This is Gatekeeper's quarantine attribute on an unsigned app. Remove it:

```bash
xattr -d com.apple.quarantine /Applications/Servel.app
```

Releases are not notarized yet, so this is needed after each manual install.

### PHP: `cURL error 60: SSL certificate problem`

PHP on Windows has no certificate list. Open **Settings → PHP & Node → PHP extensions** and click **Fix HTTPS certificates** for the PHP version you use. For a single project only, you can instead add `SSL_CERT_FILE=<path to ca-bundle.crt>` to its **Environment**.

### PHP extension change was undone

PHP printed a startup error with the new setting, so Servel restored `php.ini`; the message shows PHP's own error. Usually the DLL does not match the PHP version, thread safety (TS/NTS) or architecture. Download the matching build and try again.

### Project shows "port not answering"

The process is running but nothing listens on the project's port. The app probably uses its own port: check **Logs → Projects** for the address it prints, then set that port in Edit (or make the app read `PORT`).

### Old `.test` domains still resolve (upgraded from v2.1 or older)

The `.test` domains feature was removed in v2.2. **Settings → Projects** shows a cleanup box while Servel's hosts-file entries or the old local HTTPS certificate are still present: **Remove from hosts** (asks for admin rights) and **Remove certificate**.

### Service stays stuck on "starting"

Check the Logs viewer for that service. Common causes: the image is still downloading on first run, a port conflict, or no disk space left. Servel marks a service "up" after 120 seconds even if its health check never passes, and writes a warning to the log.

---

## Contributing

Bug reports, feature requests, and pull requests are welcome.

- **Issues:** [github.com/devhardiyanto/servel/issues](https://github.com/devhardiyanto/servel/issues)
- **Discussions:** [github.com/devhardiyanto/servel/discussions](https://github.com/devhardiyanto/servel/discussions)

Please include your OS, Servel version, and relevant log output when filing a bug.

---

## Development

```bash
npm install
npm run tauri dev
```

Requirements: Node.js 20 LTS, Rust stable (via [rustup](https://rustup.rs/)), and platform-specific build tools (Visual Studio Build Tools "Desktop development with C++" on Windows; Xcode Command Line Tools on macOS; standard `build-essential` + `libwebkit2gtk-4.1-dev` on Linux).

---

## Privacy policy

Servel does not collect, transmit, or store any personal data. All configuration is saved locally on your machine only. There is no telemetry and no analytics. On launch Servel asks GitHub whether a newer release exists; no data about you or your machine is sent beyond that request. All other network calls are initiated by you (image pulls, version installs, downloading an update).

---

## Code signing policy

Free code signing on Windows provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

- Committers and reviewers: [@devhardiyanto](https://github.com/devhardiyanto)
- Approvers: [@devhardiyanto](https://github.com/devhardiyanto)

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

---

## License

MIT. See [LICENSE](LICENSE) for details.
