# CodexBar for Android

An Android client for the CodexBar **self-hosted sync server** that ships in this
repository (see the `sync-server` section of the top-level `README.md`). It lives
in `android/` inside the `codexbar-plasmoid` repo and is a thin, read-only viewer:
it renders the usage snapshots other CodexBar devices have published to the server.

- Compose-only UI (no AppCompat, no XML layouts)
- `namespace` / `applicationId`: `org.slopfire.codexbar`
- `minSdk` 26, `targetSdk` / `compileSdk` 37, AGP `9.5.0-alpha03`
- Activity: `org.slopfire.codexbar.MainActivity`

## Server routes used

The app is a consumer of the sync server. It reads, and only deletes on an
explicit "forget device" action:

| Method & path | Auth | Purpose |
| --- | --- | --- |
| `GET /health` | none | Liveness / version probe before connecting |
| `GET /v1/usage?provider=<id>` | `Authorization: Bearer <token>` | Merged usage entries for one provider |
| `GET /v1/devices` | `Authorization: Bearer <token>` | Devices that have published snapshots |
| `DELETE /v1/snapshot/<device>` | `Authorization: Bearer <token>` | Forget a renamed/reinstalled device |

Every `/v1` route requires `Authorization: Bearer <token>`. The app never
publishes snapshots (`PUT /v1/snapshot/<device>` is a writer route and is unused
here); it only reads, plus the explicit forget-device DELETE above.

## Run the server

On an always-on machine (NAS, home server, or a Tailscale/WireGuard host):

```sh
codexbar-plasmoid sync-server --bind 0.0.0.0 --port 8188 --token <secret>
# print the effective token without starting the server:
codexbar-plasmoid sync-server --show-token
```

Then enter the base URL (for example `http://10.0.0.5:8188`) and the token in the
app's server settings.

## Build

A `local.properties` with an `sdk.dir` pointing at your Android SDK is required
(Gradle will not build without it):

```properties
sdk.dir=/home/you/Android/Sdk
```

```sh
cd android
./gradlew :app:assembleDebug
```

## Home screen widgets

Three Glance widgets read the same cached payload as the app (one network fetch
serves every widget instance), and each receiver refreshes in the background
after the platform's `updatePeriodMillis` tick:

| Widget | Cells | Shows |
| --- | --- | --- |
| `CompactWidget` | 3x1 | The provider closest to running out: emphasized percent, bar, pace verdict |
| `ProvidersWidget` | 4x2 | Every merged provider, tightest first, plus a tap-to-refresh action |
| `SpendWidget` | 3x2 | Today and 30-day cost, top model, per-provider breakdown |

They are pinnable from **Settings → Home screen widgets** via
`requestPinAppWidget`, which shows the launcher's own confirmation.

### Configuring a widget

Each widget instance is configured independently, and settings live in that
instance's Glance preferences — so a panel widget can watch one provider while a
desktop widget shows everything.

Two ways in:

- **Settings → Home screen widgets → Configure** on any widget that is already
  placed. This is the reliable path: the launcher's pin flow deliberately skips
  `android:configure`, and long-press → Edit only exists from Android 12.
- **Long-press the widget → Edit**, which the launcher offers because each
  provider declares `android:widgetFeatures="reconfigurable"`. Adding from the
  launcher's own widget picker opens the configuration directly.

The options are provider selection (multi-select, "All" clears the filter), order
(tightest first or name), an explicit row count for the providers widget, the
reset/pace caption line, and the palette — Material You, Light, or Dark, so a
widget can be pinned to an appearance instead of following the system.

The widget header shows the product name and how fresh the payload is, tinted once
it goes stale or a refresh fails. It deliberately does **not** show the sync server
address: that is noise on a home screen.

Widget-specific notes:

- Colors come from the same `codexBarColorScheme` the app uses, so a widget and
  the app share one Material You palette. Glance's Material 3 provider omits the
  Expressive `surface container` roles, so the container fill is supplied
  explicitly from the real `ColorScheme` instead of substituting a wrong role.
- Provider badges draw the same brand icons as the app: white silhouettes
  rasterized from the plasmoid's `ProviderIcon-*.svg` set, tinted with the
  provider's accent color. See **Provider icons** below.
- Each widget reads its own settings from Glance preferences and resolves its
  palette per render through a `CompositionLocal`, so the shell, bars and tiles
  agree without every signature taking a palette.
- `ProvidersWidget` and `SpendWidget` use `SizeMode.Exact` so `LocalSize` reports
  the real size and the row count adapts. Under the default `SizeMode.Single`,
  `LocalSize` is the *minimum* size from the provider info, which collapses the
  list to a single row.
- Motion is deliberately absent: widgets render as static `RemoteViews`, and M3
  Expressive springs have no app-widget equivalent.

## Unit tests

```sh
cd android
./gradlew :app:testDebugUnitTest
```

69 JVM unit tests cover the JSON contract (a full merged payload plus missing,
mistyped and error-shaped fields), URL normalization and path encoding, the HTTP
client against a real `com.sun.net.httpserver.HttpServer` (auth header, query
parameter, 401/500/empty-body/unreachable mapping), and the repository's
best-effort health and device reads.

## What it shows

- **Usage** — a summary strip (provider count, tightest remaining percent,
  snapshot age), provider filter and sort chips, and one card per merged entry
  with remaining-limit bars, pace ticks and verdicts, credits, reset credits,
  token cost, contributing devices, and soft/hard degraded-state notes.
- **Detail** — limits, reset credits, spend tiles, a tappable 30-day history
  chart with a per-day breakdown, a merged model breakdown with share bars,
  contributing devices, and raw metadata.
- **Devices** — server health and version, every publishing device with its
  entry count and received/generated times, an explicit forget-device action,
  and a summary of what the server stores.
- **Settings** — server address, bearer token (masked, with a reveal toggle),
  device name, auto-refresh cadence, connection status, and security notes.

## Notable dependency pins

- `material3 = "1.5.0-alpha17"` — the Material 3 Expressive *components*
  (`MaterialExpressiveTheme`, `MotionScheme`, `LoadingIndicator`,
  `ShortNavigationBar`, `MaterialShapes`) only exist in the 1.5.0 alpha line;
  1.4.0 has the expressive theme but none of the components.
- `glance = "1.2.0"` — the Compose-first app-widget toolkit; `glance-material3`
  supplies `ColorProviders(light, dark)` from a real `ColorScheme`. Pinned at
  1.2.0 because it requires `kotlin-stdlib` 2.0.21, which the AGP-bundled
  compiler reads.
- Provider icons and accent colors are bundled locally (copied from the
  plasmoid's `contents/images` and `ProviderSwitcher.qml`), so the app renders
  provider identity with no network dependency beyond the sync server itself.

## Provider icons

`res/drawable-xxhdpi/provider_<id>.png` — 46 white silhouettes rasterized from
the plasmoid's own icon set, so the app, the home screen widgets and the desktop
widget all show the same art. They are tinted at render time with the provider's
accent color, which is required rather than cosmetic: the source art is white and
would be invisible on a light surface, and the accent *is* the provider's identity
color in this project.

Regenerate them after the plasmoid gains or changes an icon:

```sh
python3 android/scripts/generate-provider-icons.py   # needs inkscape + ImageMagick
```

Two upstream quirks are handled by the script, and both silently produce blank
images if missed: `fill="currentColor"` (no color context for Inkscape) and
`width="1em" height="1em"` (no resolvable pixel size). The script fails loudly on
a blank export instead of shipping an invisible icon.

A provider with no bundled icon falls back to an initials monogram, resolved by
`providerIconRes()` in `ui/theme/ProviderStyle.kt`.

## Signing

The release build is signed from `keystore/`, which is **not** in version
control (mode 0700, files 0600):

```
keystore/codexbar-release.p12     # PKCS#12, RSA 4096, self-signed, 10000 days
keystore/keystore.properties      # store/key passwords
```

`app/build.gradle.kts` reads `keystore.properties` and applies the config when it
exists; without it the release build stays unsigned rather than failing, so a
fresh clone or CI without the secret can still run `assembleRelease`.

```sh
./gradlew :app:assembleRelease
~/Android/Sdk/build-tools/37.0.0/apksigner verify --print-certs \
  app/build/outputs/apk/release/app-release.apk
```

To rotate to your own key, replace both files — but note that Android refuses an
update whose signing certificate differs from the installed app, so an existing
install must be uninstalled first.

## Design tokens

UI values come from the project skill `.agents/skills/m3-expressive/`, not from
memory. The two that changed the app:

- **Corner radius scale** (`references/tokens.md`) — the published ten-step scale
  is 0 / 4 / 8 / 12 / 16 / 20 / 28 / 32 / 48 / full. An earlier revision of this
  app used invented values (14 / 22 / 36dp); those are not M3 steps.
- **Cards use `CornerMedium` (12dp)** (`references/component-tokens.md`) — all
  three card variants do. Large radii clip the content of an information-dense
  container. Nested corners follow `inner = outer − padding`.

Emphasized typography is not a Compose API in this Material version, so
`ui/theme/ExpressiveType.kt` implements it from the published token values and
applies it only where M3 says it belongs (headlines, the headline number,
selection, badges) — never as a whole type scale.

## Security notes

- The sync server speaks **plain HTTP**, and the bearer token travels on every
  `/v1` request. Treat the endpoint as LAN/VPN-only (home LAN or a
  Tailscale/WireGuard tunnel) and do not expose it directly to the internet.
  Cleartext is enabled deliberately via `res/xml/network_security_config.xml`;
  the manifest does **not** set `android:usesCleartextTraffic`.
- The token is stored only in the app-private SharedPreferences file
  `codexbar-sync.xml`. It is excluded from both cloud backup and device-to-device
  transfer (`res/xml/backup_rules.xml` and `res/xml/data_extraction_rules.xml`),
  so it never leaves the device.
- The app is read-only against the server: it fetches usage/devices and only
  deletes a device snapshot when the user explicitly asks to forget it.
