import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QtControls
import org.kde.kirigami as Kirigami
import org.kde.plasma.components 3.0 as PlasmaComponents3
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.extras as PlasmaExtras
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.plasma.plasmoid

PlasmoidItem {
    id: root

    property var snapshot: ({ ok: false, entries: [] })
    property var cliUpdateInfo: ({ ok: true, installedVersion: "", latestVersion: "", needsUpdate: false, updated: false, error: "" })
    property bool loading: false
    property string lastError: ""
    // An empty selection means "show every provider". Keep this as an array
    // rather than a single id so provider chips can be combined as filters.
    // Restored from plasmoid.configuration.selectedEntryIds on startup.
    property var selectedEntryIds: []
    property bool multiProviderSelectionEnabled: plasmoid.configuration.allowMultiProviderSelection === true
    property string activeCommand: ""
    property string previousCommand: ""
    readonly property var entries: snapshot.entries || []
    readonly property var effectiveSelectedEntryIds: multiProviderSelectionEnabled
        ? selectedEntryIds
        : selectedEntryIds.slice(0, 1)
    readonly property var visibleEntries: effectiveSelectedEntryIds.length > 0
        ? entries.filter(function(entry) { return effectiveSelectedEntryIds.indexOf(entry.id) !== -1; })
        : entries
    readonly property var defaultEntry: entries.length > 0 ? entries[0] : null
    readonly property var primaryEntry: visibleEntries.length > 0 ? visibleEntries[0] : null
    readonly property int refreshInterval: Math.max(60, plasmoid.configuration.refreshIntervalSeconds || 300)

    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation
    toolTipMainText: primaryEntry
        ? codexBar.providerName(primaryEntry.provider) + " · " + tooltipText()
        : i18n("CodexBar")
    toolTipSubText: ""
    Plasmoid.contextualActions: [
        PlasmaCore.Action {
            text: i18n("Refresh")
            icon.name: "view-refresh"
            onTriggered: root.refreshNow(true)
        }
    ]

    Component.onCompleted: {
        selectedEntryIds = loadSelectedEntryIds();
        if (!multiProviderSelectionEnabled && selectedEntryIds.length > 1) {
            selectedEntryIds = selectedEntryIds.slice(0, 1);
        }
        refreshNow(false);
    }
    onRefreshIntervalChanged: refreshTimer.restart()
    onSelectedEntryIdsChanged: persistSelectedEntryIds()
    onMultiProviderSelectionEnabledChanged: {
        if (!multiProviderSelectionEnabled && selectedEntryIds.length > 1) {
            selectedEntryIds = selectedEntryIds.slice(0, 1);
        }
    }

    Timer {
        id: refreshTimer
        interval: root.refreshInterval * 1000
        repeat: true
        running: true
        triggeredOnStart: false
        onTriggered: root.refreshNow(false)
    }

    QtObject {
        id: codexBar

        function localPath(url) {
            return decodeURIComponent(String(url).replace(/^file:\/\//, ""));
        }

        function quote(value) {
            return "'" + String(value).replace(/'/g, "'\"'\"'") + "'";
        }

        function command(forceRefresh) {
            const script = localPath(Qt.resolvedUrl("../code/codexbar-plasmoid-helper.mjs"));
            const parts = [
                quote(script),
                "--cli", quote(plasmoid.configuration.cliPath || "codexbar"),
                "--providers", quote(plasmoid.configuration.providerConfigs || ""),
                "--provider", quote(plasmoid.configuration.provider || "all"),
                "--source", quote(plasmoid.configuration.source || "auto"),
                "--timeout", quote(plasmoid.configuration.requestTimeoutSeconds || 45),
                "--account", quote(plasmoid.configuration.account || ""),
                "--accountIndex", quote(plasmoid.configuration.accountIndex || 0),
                "--allAccounts", quote(plasmoid.configuration.allAccounts ? "true" : "false"),
                "--status", quote(plasmoid.configuration.includeStatus ? "true" : "false"),
                "--cost", quote(plasmoid.configuration.includeCost ? "true" : "false"),
                "--credits", quote(plasmoid.configuration.showCredits ? "true" : "false"),
                "--anonymize-emails", quote(plasmoid.configuration.anonymizeEmail ? "true" : "false"),
                "--auto-update", quote(plasmoid.configuration.autoUpdateCli ? "true" : "false"),
                "--tag", quote(plasmoid.configuration.cliUpdateChannel || "latest"),
                "--cache-seconds", quote(plasmoid.configuration.shareProviderFetches === false ? 0 : root.refreshInterval),
                "--sync-providers", quote(plasmoid.configuration.syncProviders === true ? "true" : "false"),
                "--force", quote(forceRefresh ? "true" : "false")
            ];
            return parts.join(" ");
        }

        function providerName(provider) {
            const names = {
                codex: "Codex",
                openai: "OpenAI",
                azureopenai: "Azure OpenAI",
                claude: "Claude",
                gemini: "Gemini",
                antigravity: "Antigravity",
                cursor: "Cursor",
                opencode: "OpenCode",
                opencodego: "OpenCode Go",
                alibaba: "Alibaba Coding",
                alibabatokenplan: "Alibaba Token",
                zai: "z.ai",
                factory: "Factory",
                copilot: "Copilot",
                minimax: "MiniMax",
                manus: "Manus",
                vertexai: "Vertex AI",
                kilo: "Kilo",
                kiro: "Kiro",
                augment: "Augment",
                jetbrains: "JetBrains",
                kimi: "Kimi",
                kimik2: "Kimi K2",
                moonshot: "Moonshot",
                amp: "Amp",
                t3chat: "T3 Chat",
                ollama: "Ollama",
                synthetic: "Synthetic",
                openrouter: "OpenRouter",
                elevenlabs: "ElevenLabs",
                warp: "Warp",
                windsurf: "Windsurf",
                perplexity: "Perplexity",
                mimo: "MiMo",
                doubao: "Doubao",
                abacus: "Abacus",
                mistral: "Mistral",
                deepseek: "DeepSeek",
                codebuff: "Codebuff",
                crof: "Crof",
                venice: "Venice",
                commandcode: "Command Code",
                stepfun: "StepFun",
                bedrock: "Bedrock",
                grok: "Grok",
                groq: "Groq",
                llmproxy: "LLM Proxy",
                deepgram: "Deepgram",
                devin: "Devin"
            };
            return names[provider] || String(provider || "CodexBar");
        }

        function color(provider) {
            const settings = providerConfig(provider);
            if (settings && settings.compactColor) {
                return settings.compactColor;
            }
            const colors = {
                codex: "#4b929b",
                openai: "#398979",
                azureopenai: "#397fb7",
                claude: "#b57861",
                gemini: "#8972b5",
                antigravity: "#55976b",
                cursor: "#3c9487",
                opencode: "#477fc2",
                opencodego: "#477fc2",
                alibaba: "#bd7434",
                alibabatokenplan: "#bd7434",
                zai: "#b96170",
                factory: "#bd6e4c",
                copilot: "#8c68b7",
                minimax: "#bd684e",
                vertexai: "#4c7fb8",
                kilo: "#b87243",
                kiro: "#b57b32",
                augment: "#666db0",
                jetbrains: "#ae628a",
                moonshot: "#4b72b4",
                perplexity: "#438f8b",
                deepseek: "#5879b8",
                grok: "#3d8d76",
                groq: "#b86c55",
                openrouter: "#4d88ad",
                llmproxy: "#4a9173",
                devin: "#6769ad"
            };
            return colors[provider] || Kirigami.Theme.highlightColor;
        }

        function providerConfig(provider) {
            const normalized = normalizeProviderId(provider);
            const configs = parseProviderConfigs();
            for (const config of configs) {
                if (normalizeProviderId(config.provider) === normalized) {
                    return config;
                }
            }
            return null;
        }

        function parseProviderConfigs() {
            try {
                const parsed = JSON.parse(String(plasmoid.configuration.providerConfigs || ""));
                return Array.isArray(parsed) ? parsed : [];
            } catch (error) {
                return [];
            }
        }

        function normalizeProviderId(provider) {
            const normalized = String(provider || "").toLowerCase().replace(/[-_]/g, "");
            const aliases = {
                abacusai: "abacus",
                alibabacodingplan: "alibaba",
                groqcloud: "groq"
            };
            return aliases[normalized] || normalized;
        }

        // Remaining-limit tint: white at 100% left, pure red at 0%, smooth RGB lerp.
        function remainingLimitColor(percentLeft) {
            const value = Number(percentLeft);
            if (!Number.isFinite(value)) {
                return Kirigami.Theme.negativeTextColor;
            }
            const t = Math.max(0, Math.min(100, value)) / 100;
            // white (1,1,1) → red (1,0,0)
            return Qt.rgba(1, t, t, 1);
        }

        function compactBarColor(provider, percentLeft) {
            const tint = plasmoid.configuration.compactBarsTint || "provider";
            if (tint === "threshold") {
                return remainingLimitColor(percentLeft);
            }
            if (tint === "theme") {
                return Kirigami.Theme.textColor;
            }
            return color(provider);
        }

        function percent(value) {
            return Number.isFinite(Number(value)) ? Math.round(Number(value)) + "%" : "—";
        }

        function money(value, code) {
            if (!Number.isFinite(Number(value))) {
                return "—";
            }
            const val = Number(value);
            let sym = "";
            const currency = String(code || "USD").toUpperCase();
            if (currency === "USD") {
                sym = "$";
            } else if (currency === "EUR") {
                sym = "€";
            } else if (currency === "GBP") {
                sym = "£";
            } else if (currency === "JPY") {
                sym = "¥";
            } else {
                sym = currency + " ";
            }
            let decimals = 0;
            if (val < 1) {
                decimals = 2;
            } else if (val < 10) {
                decimals = 1;
            }
            return sym + val.toLocaleString(Qt.locale(), "f", decimals);
        }

        function tokens(value) {
            if (!Number.isFinite(Number(value))) {
                return "";
            }
            return Math.round(Number(value)).toLocaleString(Qt.locale(), "f", 0) + " " + i18n("tokens");
        }

        function relativeTime(value) {
            const date = new Date(value);
            if (!Number.isFinite(date.getTime())) {
                return "";
            }
            const seconds = Math.max(0, Math.round((Date.now() - date.getTime()) / 1000));
            if (seconds < 60) {
                return i18n("now");
            }
            if (seconds < 3600) {
                return i18np("%1 min ago", "%1 min ago", Math.round(seconds / 60));
            }
            if (seconds < 86400) {
                return i18np("%1 hr ago", "%1 hrs ago", Math.round(seconds / 3600));
            }
            return i18np("%1 day ago", "%1 days ago", Math.round(seconds / 86400));
        }

        function compactValue(entry) {
            if (!entry) {
                return "—";
            }
            const metric = plasmoid.configuration.compactMetric || "lowest";
            if (metric === "credits" && entry.creditsRemaining !== null) {
                return Number(entry.creditsRemaining).toLocaleString(Qt.locale(), "f", 1);
            }
            if (metric === "todayCost" && entry.tokenUsage) {
                return money(entry.tokenUsage.sessionCostUSD, entry.tokenUsage.currencyCode);
            }
            const rows = entry.rows || [];
            if (metric === "session" && rows.length > 0) {
                return percent(rows[0].percentLeft);
            }
            if (metric === "weekly" && rows.length > 1) {
                return percent(rows[1].percentLeft);
            }
            let lowest = null;
            for (const row of rows) {
                const value = Number(row.percentLeft);
                if (Number.isFinite(value) && (lowest === null || value < lowest)) {
                    lowest = value;
                }
            }
            if (lowest !== null) {
                return percent(lowest);
            }
            if (entry.creditsRemaining !== null) {
                return Number(entry.creditsRemaining).toLocaleString(Qt.locale(), "f", 1);
            }
            if (entry.tokenUsage) {
                return money(entry.tokenUsage.sessionCostUSD, entry.tokenUsage.currencyCode);
            }
            return "—";
        }

        function compactBarPercent(entry) {
            if (!entry) {
                return 0;
            }
            const metric = plasmoid.configuration.compactMetric || "lowest";
            const rows = entry.rows || [];
            if (metric === "session" && rows.length > 0) {
                return Number(rows[0].percentLeft);
            }
            if (metric === "weekly" && rows.length > 1) {
                return Number(rows[1].percentLeft);
            }
            let lowest = null;
            for (const row of rows) {
                const value = Number(row.percentLeft);
                if (Number.isFinite(value) && (lowest === null || value < lowest)) {
                    lowest = value;
                }
            }
            if (lowest !== null) {
                return lowest;
            }
            return rows.length > 0 ? Number(rows[0].percentLeft) : 0;
        }

        function compactBarLimit(config) {
            const value = Number(config && config.compactBarLimit);
            return Number.isFinite(value) ? Math.max(1, Math.min(4, Math.round(value))) : 4;
        }

        function selectedCompactBarIds(config) {
            try {
                const parsed = JSON.parse(String(config && config.compactBarIds || "[]"));
                return Array.isArray(parsed) ? parsed.map(String) : [];
            } catch (error) {
                return [];
            }
        }

        function updateCompactBarCatalog(entries) {
            let catalog = {};
            try {
                const cached = JSON.parse(String(plasmoid.configuration.compactBarCatalog || "{}"));
                if (cached && typeof cached === "object" && !Array.isArray(cached)) {
                    catalog = cached;
                }
            } catch (error) {
                catalog = {};
            }
            for (const entry of entries || []) {
                const provider = normalizeProviderId(entry.provider);
                if (!catalog[provider]) {
                    catalog[provider] = [];
                }
                for (const row of entry.rows || []) {
                    const id = String(row.id || "");
                    if (!id) {
                        continue;
                    }
                    const title = String(row.title || id);
                    let existing = -1;
                    for (let index = 0; index < catalog[provider].length; index += 1) {
                        if (String(catalog[provider][index].id || "") === id) {
                            existing = index;
                            break;
                        }
                    }
                    if (existing === -1) {
                        catalog[provider].push({ id: id, title: title });
                    } else {
                        catalog[provider][existing] = { id: id, title: title };
                    }
                }
            }
            const serialized = JSON.stringify(catalog);
            if (serialized !== String(plasmoid.configuration.compactBarCatalog || "{}")) {
                plasmoid.configuration.compactBarCatalog = serialized;
            }
        }

        function compactBarRows(entry) {
            const output = [];
            const rows = entry && entry.rows ? entry.rows : [];
            const config = entry ? providerConfig(entry.provider) : null;
            const selectedIds = selectedCompactBarIds(config);
            let filteredRows = selectedIds.length > 0
                ? rows.filter(function(row) { return selectedIds.indexOf(String(row.id || "")) !== -1; })
                : rows.slice(0, compactBarLimit(config));
            // If a provider replaced its row IDs, keep the tray useful until
            // the user reviews the newly discovered choices.
            if (filteredRows.length === 0 && rows.length > 0) {
                filteredRows = rows.slice(0, compactBarLimit(config));
            }
            for (let index = 0; index < filteredRows.length; index += 1) {
                const row = filteredRows[index];
                const percentLeft = Number(row.percentLeft);
                if (!Number.isFinite(percentLeft)) {
                    continue;
                }
                output.push({
                    id: String(row.id || ["primary", "secondary", "tertiary"][index] || ""),
                    title: String(row.title || ""),
                    percentLeft: Math.max(0, Math.min(100, percentLeft)),
                    color: compactBarColor(entry.provider, percentLeft)
                });
            }
            if (output.length === 0 && filteredRows.length > 0) {
                const percentLeft = compactBarPercent(entry);
                if (Number.isFinite(percentLeft)) {
                    output.push({
                        id: "primary",
                        title: providerName(entry.provider),
                        percentLeft: Math.max(0, Math.min(100, percentLeft)),
                        color: compactBarColor(entry.provider, percentLeft)
                    });
                }
            }
            if (output.length === 0 && entry && (entry.creditsRemaining !== null || entry.tokenUsage)) {
                output.push({
                    kind: "credits",
                    title: providerName(entry.provider),
                    valueText: entry.creditsRemaining !== null
                        ? Number(entry.creditsRemaining).toLocaleString(Qt.locale(), "f", entry.creditsRemaining < 10 ? 1 : 0)
                        : money(entry.tokenUsage.sessionCostUSD, entry.tokenUsage.currencyCode),
                    color: compactBarColor(entry.provider, 100)
                });
            }
            return output;
        }

        function compactBarEntries() {
            const mode = plasmoid.configuration.compactBarsProviders || "default";
            if (mode === "all") {
                return root.entries.filter(function(entry) {
                    const config = providerConfig(entry.provider);
                    return !config || config.showInCompactAll !== false;
                });
            }
            if (mode === "selected") {
                return root.effectiveSelectedEntryIds.length > 0 ? root.visibleEntries : [];
            }
            return root.defaultEntry ? [root.defaultEntry] : [];
        }

        function compactBarItems() {
            const items = [];
            const sourceEntries = compactBarEntries();
            for (const entry of sourceEntries) {
                const rows = compactBarRows(entry);
                if (rows.length === 0) {
                    continue;
                }
                items.push({
                    provider: entry.provider,
                    title: providerName(entry.provider),
                    rows
                });
            }
            return items;
        }
    }

    Plasma5Support.DataSource {
        id: executable
        engine: "executable"
        connectedSources: []
        interval: 0
        onNewData: function(sourceName, data) {
            root.loading = false;
            disconnectSource(sourceName);
            const output = String(data.stdout || data["stdout"] || "").trim();
            if (!output.length) {
                root.lastError = i18n("CodexBar returned no data");
                return;
            }
            try {
                const parsed = JSON.parse(output);
                root.snapshot = parsed;
                codexBar.updateCompactBarCatalog(parsed.entries || []);
                root.lastError = parsed.ok === false ? (parsed.error || i18n("CodexBar refresh failed")) : "";
                if (parsed.cliUpdate && (parsed.cliUpdate.updated || parsed.cliUpdate.error)) {
                    root.cliUpdateInfo = parsed.cliUpdate;
                }
                // Drop stale ids only when we have a real entry list. An empty
                // or failed refresh must not wipe the remembered selection.
                if (root.selectedEntryIds.length > 0) {
                    const availableIds = (parsed.entries || []).map(function(entry) { return entry.id; });
                    if (availableIds.length > 0) {
                        root.selectedEntryIds = root.selectedEntryIds.filter(function(entryId) {
                            return availableIds.indexOf(entryId) !== -1;
                        });
                    }
                }
            } catch (error) {
                root.lastError = String(error) + "\n" + output.slice(0, 500);
            }
        }
    }

    Plasma5Support.DataSource {
        id: updater
        engine: "executable"
        connectedSources: []
        interval: 0
        onNewData: function(sourceName, data) {
            updater.disconnectSource(sourceName);
            const output = String(data.stdout || data["stdout"] || "").trim();
            if (!output.length) {
                root.cliUpdateInfo = { ok: false, error: i18n("CLI updater returned no data") };
                return;
            }
            try {
                root.cliUpdateInfo = JSON.parse(output);
            } catch (error) {
                root.cliUpdateInfo = { ok: false, error: String(error) + "\n" + output.slice(0, 500) };
            }
        }
    }

    function refreshNow(forceRefresh) {
        const command = codexBar.command(forceRefresh === true);
        if (previousCommand.length > 0) {
            executable.disconnectSource(previousCommand);
        }
        previousCommand = command;
        activeCommand = command;
        loading = true;
        lastError = "";
        executable.connectSource(command);
    }

    function loadSelectedEntryIds() {
        try {
            const parsed = JSON.parse(String(plasmoid.configuration.selectedEntryIds || "[]"));
            if (!Array.isArray(parsed)) {
                return [];
            }
            return parsed.map(function(entryId) { return String(entryId); }).filter(function(entryId) {
                return entryId.length > 0;
            });
        } catch (error) {
            return [];
        }
    }

    function persistSelectedEntryIds() {
        const serialized = JSON.stringify(selectedEntryIds || []);
        if (serialized !== String(plasmoid.configuration.selectedEntryIds || "[]")) {
            plasmoid.configuration.selectedEntryIds = serialized;
        }
    }

    function toggleEntrySelection(entryId) {
        if (!multiProviderSelectionEnabled) {
            selectedEntryIds = selectedEntryIds.length === 1 && selectedEntryIds[0] === entryId
                ? []
                : [entryId];
            return;
        }
        const selected = selectedEntryIds.slice();
        const index = selected.indexOf(entryId);
        if (index === -1) {
            selected.push(entryId);
        } else {
            selected.splice(index, 1);
        }
        // Returning to an empty selection intentionally restores the
        // unfiltered, all-provider view.
        selectedEntryIds = selected;
    }

    function checkCliUpdate() {
        const script = codexBar.localPath(Qt.resolvedUrl("../code/codexbar-cli-updater.mjs"));
        updater.connectSource(codexBar.quote(script) + " --action check --tag " + codexBar.quote(plasmoid.configuration.cliUpdateChannel || "latest"));
    }

    function updateCliNow() {
        const script = codexBar.localPath(Qt.resolvedUrl("../code/codexbar-cli-updater.mjs"));
        updater.connectSource(codexBar.quote(script) + " --action update --tag " + codexBar.quote(plasmoid.configuration.cliUpdateChannel || "latest"));
    }

    function cliUpdateLabel() {
        const info = root.cliUpdateInfo;
        if (!info || info.error) {
            return info && info.error ? info.error : "";
        }
        if (info.updated) {
            if (info.previousVersion && info.installedVersion && info.previousVersion !== info.installedVersion) {
                return i18n("Updated CLI %1 → %2", info.previousVersion, info.installedVersion);
            }
            return i18n("Updated CLI to %1", info.installedVersion || info.latestVersion);
        }
        if (info.needsUpdate) {
            return i18n("CLI %1 available (installed %2)", info.latestVersion, info.installedVersion || i18n("none"));
        }
        if (info.installedVersion) {
            return i18n("CLI %1 is up to date", info.installedVersion);
        }
        return "";
    }

    function tooltipText() {
        if (loading) {
            return i18n("Refreshing");
        }
        if (lastError.length > 0) {
            return lastError;
        }
        if (!primaryEntry) {
            return i18n("No usage data");
        }
        return codexBar.compactValue(primaryEntry) + " · " + codexBar.relativeTime(primaryEntry.updatedAt);
    }

    compactRepresentation: CompactRepresentation {
        entry: root.primaryEntry
        loading: root.loading
        errorText: root.lastError
        providerId: root.primaryEntry ? root.primaryEntry.provider : ""
        providerName: root.primaryEntry ? codexBar.providerName(root.primaryEntry.provider) : i18n("CodexBar")
        accentColor: root.primaryEntry ? codexBar.color(root.primaryEntry.provider) : Kirigami.Theme.highlightColor
        valueText: root.primaryEntry ? codexBar.compactValue(root.primaryEntry) : "—"
        displayMode: plasmoid.configuration.compactDisplay || "bars-first"
        showMetricText: plasmoid.configuration.compactShowMetric !== false
        usageRows: root.primaryEntry && root.primaryEntry.rows ? root.primaryEntry.rows : []
        barItems: codexBar.compactBarItems()
        providerBarWidth: plasmoid.configuration.compactProviderBarWidth || 18
        onClicked: root.expanded = !root.expanded
    }

    fullRepresentation: PlasmaExtras.Representation {
        id: representation
        Layout.minimumWidth: Kirigami.Units.gridUnit * 18
        Layout.minimumHeight: Kirigami.Units.gridUnit * 20
        Layout.preferredWidth: Kirigami.Units.gridUnit * 24
        Layout.preferredHeight: Kirigami.Units.gridUnit * 32
        collapseMarginsHint: true

        contentItem: ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.smallSpacing

                PlasmaExtras.Heading {
                    Layout.fillWidth: true
                    level: 3
                    text: i18n("CodexBar")
                    elide: Text.ElideRight
                }

                PlasmaComponents3.BusyIndicator {
                    Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                    Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
                    running: root.loading
                    visible: root.loading
                }

                PlasmaComponents3.ToolButton {
                    icon.name: "view-refresh"
                    text: i18n("Refresh")
                    display: QtControls.AbstractButton.IconOnly
                    enabled: !root.loading
                    onClicked: root.refreshNow(true)
                }

                PlasmaComponents3.ToolButton {
                    icon.name: "configure"
                    text: i18n("Configure")
                    display: QtControls.AbstractButton.IconOnly
                    onClicked: plasmoid.internalAction("configure").trigger()
                }
            }

            ProviderSwitcher {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.smallSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                entries: root.entries
                selectedEntryIds: root.effectiveSelectedEntryIds
                colorForProvider: function(provider) { return codexBar.color(provider); }
                onEntrySelected: function(entryId) {
                    root.toggleEntrySelection(entryId);
                }
            }

            PlasmaComponents3.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.smallSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                visible: root.lastError.length > 0
                text: root.lastError
                color: Kirigami.Theme.negativeTextColor
                wrapMode: Text.Wrap
                maximumLineCount: 5
                elide: Text.ElideRight
            }

            PlasmaComponents3.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.smallSpacing
                Layout.rightMargin: Kirigami.Units.smallSpacing
                visible: !root.loading && root.lastError.length === 0 && root.visibleEntries.length === 0
                text: i18n("No provider data")
                color: Kirigami.Theme.disabledTextColor
                horizontalAlignment: Text.AlignHCenter
            }

            QtControls.ScrollView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                contentWidth: availableWidth
                clip: true

                ColumnLayout {
                    width: parent.width
                    spacing: Kirigami.Units.smallSpacing

                    Repeater {
                        model: root.visibleEntries

                        ProviderCard {
                            Layout.fillWidth: true
                            Layout.leftMargin: Kirigami.Units.smallSpacing
                            Layout.rightMargin: Kirigami.Units.smallSpacing
                            entry: modelData
                            providerName: codexBar.providerName(modelData.provider)
                            accentColor: codexBar.color(modelData.provider)
                            showCredits: plasmoid.configuration.showCredits
                            showHistory: plasmoid.configuration.showHistory
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.smallSpacing
                visible: root.cliUpdateLabel().length > 0

                PlasmaComponents3.Label {
                    Layout.fillWidth: true
                    text: root.cliUpdateLabel()
                    color: root.cliUpdateInfo && root.cliUpdateInfo.updated
                        ? Kirigami.Theme.positiveTextColor
                        : (root.cliUpdateInfo && root.cliUpdateInfo.error ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.disabledTextColor)
                    font: Kirigami.Theme.smallFont
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                }

                PlasmaComponents3.ToolButton {
                    icon.name: "view-refresh"
                    text: i18n("Check CLI update")
                    display: QtControls.AbstractButton.IconOnly
                    onClicked: root.checkCliUpdate()
                }

                PlasmaComponents3.ToolButton {
                    icon.name: "download"
                    text: i18n("Update CLI")
                    display: QtControls.AbstractButton.IconOnly
                    visible: root.cliUpdateInfo && root.cliUpdateInfo.needsUpdate && !root.cliUpdateInfo.updated
                    onClicked: root.updateCliNow()
                }
            }

            PlasmaComponents3.Label {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.smallSpacing
                text: root.snapshot.generatedAt ? i18n("Updated %1", codexBar.relativeTime(root.snapshot.generatedAt)) : ""
                color: Kirigami.Theme.disabledTextColor
                font: Kirigami.Theme.smallFont
                horizontalAlignment: Text.AlignRight
            }
        }
    }
}
