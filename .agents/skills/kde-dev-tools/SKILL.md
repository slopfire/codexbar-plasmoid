---
name: kde-dev-tools
description: >-
  Use when installing, packaging, debugging, or testing KDE Plasma widgets and
  QML code. Covers kpackagetool6, plasmawindowed, qmllint, plasmashell restart,
  cuttlefish icon browser, KDE file paths, QML cache clearing, debugging
  environment variables (QT_LOGGING_RULES, QML_IMPORT_TRACE), journalctl,
  CMake/ECM, ki18n, and KDE CLI utilities (kdialog, qdbus, kreadconfig6,
  kwriteconfig6, kioclient, qmlformat).
---

# KDE Plasma 6 Development Tools

## kpackagetool6

Primary CLI for managing Plasma packages (widgets, themes, wallpapers).

### Flag Reference

| Flag | Description |
|------|-------------|
| `-t, --type <type>` | Package type: `Plasma/Applet`, `Plasma/Theme`, `Plasma/Wallpaper`, etc. |
| `-i, --install <path>` | Install the package at `<path>` |
| `-u, --upgrade <path>` | Upgrade an existing package |
| `-r, --remove <name>` | Remove by `KPlugin.Id` |
| `-l, --list` | List installed packages |
| `-s, --show <name>` | Show package metadata |
| `-g, --global` | Operate on system-wide packages |
| `-p, --packageroot <path>` | Override install destination |
| `--list-types` | List all known package types |

### Common Patterns

```bash
# Install
kpackagetool6 --type Plasma/Applet --install ./plasmoid

# Upgrade (preserves user config)
kpackagetool6 --type Plasma/Applet --upgrade ./plasmoid

# Remove by plugin ID
kpackagetool6 --type Plasma/Applet --remove org.slopfire.codexbar-plasmoid

# List installed
kpackagetool6 --type Plasma/Applet --list

# Show info
kpackagetool6 --type Plasma/Applet --show org.slopfire.codexbar-plasmoid

# Dry-run install to temp root (validation)
kpackagetool6 --type Plasma/Applet --install plasmoid --packageroot /tmp/test-pkg

# Install-or-upgrade pattern
id="org.slopfire.codexbar-plasmoid"
if kpackagetool6 --type Plasma/Applet --list | grep -q "$id"; then
  kpackagetool6 --type Plasma/Applet --upgrade ./plasmoid
else
  kpackagetool6 --type Plasma/Applet --install ./plasmoid
fi
```

### Common Errors

| Error | Fix |
|-------|-----|
| `package already exists` | Use `--upgrade` instead of `--install` |
| `package does not exist` | Use `--install` first; check `--list` |
| `could not install package` | Verify `metadata.json` has `KPackageStructure: "Plasma/Applet"` |
| Unrelated warnings in `--list` | Harmless; from third-party widgets with different structure |

---

## plasmawindowed

Run plasmoids in a standalone window without panel installation.

```bash
# From absolute path (REQUIRED — relative paths fail!)
plasmawindowed /absolute/path/to/plasmoid

# From installed package ID
plasmawindowed org.slopfire.codexbar-plasmoid

# With debugging
QT_LOGGING_RULES="qt.qml.*=true" plasmawindowed /path/to/plasmoid

# With mock CLI
PATH=/tmp/mock-cli:$PATH plasmawindowed /path/to/plasmoid
```

### Gotchas

- **Relative paths treated as component IDs**: `plasmawindowed ./plasmoid` → error.
  Always use absolute path or a wrapper script.
- **Configure dialog may not open** in windowed mode. Validate config via
  `kpackagetool6` and real panel testing.
- Uses `PlasmaCore.Types.Planar` form factor.
- **No hot-reload** — must restart after QML changes.

---

## qmllint

Static analysis for QML files (Qt 6).

```bash
# Lint all QML files
qmllint plasmoid/contents/ui/*.qml plasmoid/contents/config/config.qml

# With custom import paths
qmllint -I /path/to/qml/modules file.qml

# Output as JSON
qmllint --json file.qml

# Auto-fix (Qt 6.5+)
qmllint --fix file.qml
```

### Suppression (Qt 6.5+)

```qml
// Suppress for a block:
// qmllint disable unqualified
someUnqualifiedAccess
// qmllint enable unqualified

// Single-line suppression:
property var x: someVal // qmllint disable unqualified
```

### Common Warnings

| Warning | Meaning |
|---------|---------|
| `Unresolved type` | Missing import or unknown component |
| `Property "x" not found on type "Y"` | Typo or wrong component type |
| `unqualified access` | Use `root.property` not just `property` |
| `Unused import` | Import not referenced |

---

## plasmashell Restart

```bash
# Graceful restart
kquitapp6 plasmashell && plasmashell &

# In-place replace (keeps session)
plasmashell --replace &

# Systemd restart
systemctl --user restart plasma-plasmashell.service

# Force kill + restart (last resort)
killall plasmashell && plasmashell &
```

After `kpackagetool6 --upgrade`, you **must** restart plasmashell to see panel
changes. `plasmawindowed` reads the package directory directly and doesn't need
this.

---

## Clearing QML Cache

**Essential when QML changes don't seem to take effect:**

```bash
rm -rf ~/.cache/plasmashell/qmlcache/
kquitapp6 plasmashell && plasmashell &
```

---

## KDE File Paths

### Plasmoid Locations

| Path | Scope |
|------|-------|
| `~/.local/share/plasma/plasmoids/<id>/` | User install (kpackagetool6 default) |
| `/usr/share/plasma/plasmoids/<id>/` | System-wide (`--global`) |

### XDG Directories

| Variable | Default | KDE Usage |
|----------|---------|-----------|
| `XDG_DATA_HOME` | `~/.local/share` | Plasma packages, app data |
| `XDG_CONFIG_HOME` | `~/.config` | KDE config files, widget config |
| `XDG_CACHE_HOME` | `~/.cache` | KPackage cache, QML cache |

### Cache Locations

| Path | Contents |
|------|----------|
| `~/.cache/kpackage/` | KPackage metadata cache |
| `~/.cache/plasmashell/qmlcache/` | QML bytecode cache (clear when stale) |
| `~/.cache/plasma_theme_cache/` | Theme cache |

### Config Files

```
~/.config/plasma-org.kde.plasma.desktop-appletsrc  # Panel/desktop widget config
~/.config/kdeglobals                                # Global KDE settings
~/.config/plasmarc                                  # Plasma shell config
```

---

## Debugging

### Environment Variables

| Variable | Example | Purpose |
|----------|---------|---------|
| `QT_LOGGING_RULES` | `"qt.qml.*=true"` | Enable Qt logging categories |
| `QML_IMPORT_TRACE` | `1` | Trace QML import resolution |
| `QML_IMPORT_PATH` | `/path/to/modules` | Extra QML import directories |
| `QT_QUICK_CONTROLS_STYLE` | `org.kde.desktop` | Force specific style |
| `QSG_INFO` | `1` | Scene graph rendering info |
| `QSG_VISUALIZE` | `overdraw` / `batches` / `clip` | Visual debugging |
| `QT_QUICK_BACKEND` | `software` | Force software rendering |
| `QT_DEBUG_PLUGINS` | `1` | Debug plugin loading (verbose) |
| `KDE_DEBUG` | `1` | KDE crash handler |

### QT_LOGGING_RULES Patterns

```bash
# QML warnings
export QT_LOGGING_RULES="qt.qml.*=true"

# Plasma-specific
export QT_LOGGING_RULES="kf.plasma.*=true;kf.package.*=true"

# Combined
export QT_LOGGING_RULES="qt.qml.*=true;kf.plasma.*=true"

# Suppress noisy category
export QT_LOGGING_RULES="kf.plasma.svg=false;qt.qml.*=true"
```

### Practical Debug Session

```bash
# Full QML tracing
QT_LOGGING_RULES="qt.qml.*=true" QML_IMPORT_TRACE=1 \
  plasmawindowed /path/to/plasmoid 2>&1 | tee debug.log

# Package loading issues
QT_LOGGING_RULES="kf.package.*=true" plasmawindowed org.slopfire.codexbar-plasmoid

# Import resolution
QML_IMPORT_TRACE=1 plasmawindowed /path/to/plasmoid
```

### journalctl

```bash
# Follow plasmashell logs
journalctl --user -u plasma-plasmashell.service -f

# KDE/Qt messages
journalctl --user -f | grep -E "(plasma|qt|qml)"
```

---

## qmlformat (Code Formatter)

```bash
qmlformat -i file.qml                  # Format in place
qmlformat file.qml > formatted.qml     # Format to new file
```

---

## cuttlefish — KDE Icon Browser

```bash
cuttlefish   # Launch icon browser
```

Install: part of `plasma-sdk` package.

### Common Icon Names

```
view-refresh    configure         dialog-ok        dialog-cancel
dialog-warning  dialog-error      dialog-information
list-add        list-remove       edit-delete      edit-copy
document-save   document-open     document-new
system-shutdown utilities-system-monitor preferences-system
arrow-up        arrow-down        chronometer      office-chart-bar
go-previous     go-next           go-up            go-down
```

---

## KDE CLI Utilities

### kdialog — Dialogs from Scripts

```bash
kdialog --yesno "Continue?"
kdialog --inputbox "Enter value:" "default"
kdialog --passivepopup "Done!" 5
kdialog --getopenfilename ~/ "*.qml"
kdialog --error "Something broke"
kdialog --password "Enter API key:"
```

### qdbus — D-Bus Interaction

```bash
qdbus                                  # List all services
qdbus org.kde.plasmashell              # List interfaces
qdbus org.kde.plasmashell /PlasmaShell # List methods
dbus-monitor --session "interface='org.kde.PlasmaShell'"  # Monitor signals
```

### kreadconfig6 / kwriteconfig6

```bash
kreadconfig6 --file kdeglobals --group General --key ColorScheme
kwriteconfig6 --file kdeglobals --group General --key ColorScheme --value "BreezeDark"
```

### kioclient — KIO Operations

```bash
kioclient exec /path/to/file           # Open with default app
kioclient copy src dest                 # Copy (supports sftp://, smb://)
kioclient ls sftp://server/path/        # List remote directory
```


---

## Dev Cycle Cheat Sheet

```bash
# 1. Edit QML
$EDITOR plasmoid/contents/ui/main.qml

# 2. Static validation
qmllint plasmoid/contents/ui/*.qml plasmoid/contents/config/config.qml
kpackagetool6 --type Plasma/Applet --install plasmoid --packageroot /tmp/test

# 3. Quick preview (no install needed)
./scripts/run-windowed.sh

# 4. Full install + panel test
./scripts/install-plasmoid.sh
rm -rf ~/.cache/plasmashell/qmlcache/
kquitapp6 plasmashell && plasmashell &

# 5. Debug session
QT_LOGGING_RULES="qt.qml.*=true;kf.plasma.*=true" \
  QML_IMPORT_TRACE=1 \
  plasmawindowed /absolute/path/to/plasmoid 2>&1 | tee debug.log

# 6. Find icon names
cuttlefish
```
