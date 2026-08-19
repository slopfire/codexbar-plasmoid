# Agent instructions

This file is the **entrypoint** for agents working in this repository.

## Do this after every feature

```sh
./scripts/agent-check.sh          # static + virtual Plasma (no host UI)
./scripts/install-plasmoid.sh     # install/upgrade into the user Plasma prefix
```

Quick static-only (no KWin):

```sh
./scripts/agent-check.sh --quick
```

Install as part of the check:

```sh
./scripts/agent-check.sh --install
```

## Do not interrupt the host desktop

| Prefer | Avoid |
|--------|--------|
| `./scripts/agent-check.sh` | Restarting `plasmashell` |
| `./scripts/run-virtual-plasma.sh` | Scattering host `plasmawindowed` windows |
| Logs under `/tmp/codexbar-virtual-latest/` | Host computer-use / live screenshots |
| `virt-shot` for a JPEG of the virtual session | Opening host `plasmawindowed` |

Never open host-visible `plasmawindowed` / computer-use during agent work unless
the user explicitly approves a host window in the current turn. The wrapper
enforces this with `--allow-host-window`. Visual check of the isolated session:
skill **virt-gui-verify** (`virt-shot`).

## One-liners

```sh
# Full agent validation (default)
./scripts/agent-check.sh

# Virtual plasmoid only (package path, no install required)
./scripts/run-virtual-plasma.sh --timeout 20
cat /tmp/codexbar-virtual-latest/app.log

# Form factors (plugin must be installed)
./scripts/install-plasmoid.sh
./scripts/run-virtual-plasma.sh --viewer planar --timeout 15
./scripts/run-virtual-plasma.sh --viewer horizontal --timeout 15

# Deterministic helper data
./scripts/setup-mock-cli.sh
PATH=/tmp/codexbar-plasma-mock:$PATH \
  node plasmoid/contents/code/codexbar-plasmoid-helper.mjs --provider all --timeout 5

# Providers settings + account discovery (virtual KWin, no host window)
./scripts/run-config-smoke.sh

# Human-approved host window only
./scripts/run-windowed.sh --allow-host-window
```

## Package facts

| | |
|--|--|
| Plugin id | `org.slopfire.codexbar-plasmoid` |
| Package dir | `plasmoid/` |
| Root QML | `PlasmoidItem` in `plasmoid/contents/ui/main.qml` |
| Shared env | `scripts/lib/repo-env.sh` |

**Plasma 6 launch rules**

- `plasmawindowed /abs/path/to/plasmoid` — OK  
- `plasmoidviewer -a org.slopfire.codexbar-plasmoid` after install — OK  
- `plasmoidviewer /path/to/package` — **wrong** (path is not a package arg)

## Skills (project)

| Skill | When |
|-------|------|
| `kde-plasmoid-workflow` | Install / virtual run / package layout |
| `codexbar-cli-bridge` | Helper JSON contract, mock CLI |
| `codexbar-release` | Version bump, package, KDE Store publish, push |
| `kde-plasma-api` | PlasmoidItem, Kirigami, config |
| `qml-qt-quick-reference` | QML / JS patterns |
| `kde-dev-tools` | kpackagetool6, qmllint, paths |

Also see `.agents/AGENTS.md` for QML conventions.

## CodeGraph

When a `.codegraph/` directory exists at the repository root, use CodeGraph before text search or manually reading files when locating or understanding code.
