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
    property var lastSuccessfulEntries: []
    property var cliUpdateInfo: ({ ok: true, installedVersion: "", latestVersion: "", needsUpdate: false, updated: false, error: "" })
    property bool loading: false
    property string lastError: ""
    // An empty selection means "show every provider". Keep this as an array
    // rather than a single id so provider chips can be combined as filters.
    // Restored from plasmoid.configuration.selectedEntryIds on startup.
    property var selectedEntryIds: []
    // False until Component.onCompleted finishes hydrating selection from
    // config. Prevents teardown/init from persisting an empty [] over a
    // previously saved multi-select (the main Plasma-restart amnesia bug).
    property bool selectionHydrated: false
    property bool multiProviderSelectionEnabled: plasmoid.configuration.allowMultiProviderSelection === true
    property string activeCommand: ""
    property string previousCommand: ""
    property string activeSiteLaunchCommand: ""
    property string previousSiteLaunchCommand: ""
    // True while top chips are being reordered so model rebuilds wait.
    property bool providerReorderActive: false
    property string previousSelectionStoreCommand: ""
    readonly property var entries: snapshot.entries || []
    readonly property var effectiveSelectedEntryIds: multiProviderSelectionEnabled
        ? selectedEntryIds
        : selectedEntryIds.slice(0, 1)
    // Soft-match selection tokens so a restored id like "codex:0" or a bare
    // "codex" still shows the live entry after account/source-based id changes.
    readonly property var visibleEntries: resolveVisibleEntries(entries, effectiveSelectedEntryIds)
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
        // Restore before any refresh so the first paint can keep the last
        // selection even while the CLI is still loading.
        const restored = loadSelectedEntryIds();
        selectedEntryIds = !multiProviderSelectionEnabled && restored.length > 1
            ? restored.slice(0, 1)
            : restored;
        selectionHydrated = true;
        // Re-write after hydrate so the value is marked dirty in KConfig even
        // when it matches the in-memory default and would otherwise be skipped.
        if (selectedEntryIds.length > 0) {
            persistSelectedEntryIds({ allowEmpty: false, force: true });
        }
        syncProviderEntryModel();
        // Async file fallback in case KConfig lost the keys (common when the
        // config dialog Apply rewrote General without cfg_ bindings).
        loadSelectedEntryIdsFromStore();
        refreshNow(false);
    }
    Component.onDestruction: {
        // Stop any further persists during teardown; an empty default must not
        // overwrite the last real selection on plasmashell restart.
        selectionHydrated = false;
    }
    onRefreshIntervalChanged: refreshTimer.restart()
    onSelectedEntryIdsChanged: {
        // Model rebuild only. Persistence is explicit (toggle / reconcile /
        // hydrate) so Component destruction cannot clobber saved selection.
        Qt.callLater(syncProviderEntryModel);
    }
    onMultiProviderSelectionEnabledChanged: {
        if (!multiProviderSelectionEnabled && selectedEntryIds.length > 1) {
            selectedEntryIds = selectedEntryIds.slice(0, 1);
            if (selectionHydrated) {
                persistSelectedEntryIds({ allowEmpty: false });
            }
        }
        Qt.callLater(syncProviderEntryModel);
    }
    onSnapshotChanged: Qt.callLater(syncProviderEntryModel)

    // Flat ListModel mirror of visibleEntries for the bottom provider cards.
    ListModel {
        id: providerEntryModel
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
                devin: "Devin",
                demo: "Demo"
            };
            return names[provider] || String(provider || "CodexBar");
        }

        function providerSiteUrl(provider, entry) {
            const discoveredUrl = String(entry && entry.siteUrl || "");
            if (discoveredUrl.indexOf("https://") === 0
                    || discoveredUrl.indexOf("http://127.0.0.1") === 0
                    || discoveredUrl.indexOf("http://localhost") === 0
                    || discoveredUrl.indexOf("http://[::1]") === 0) {
                return discoveredUrl;
            }
            const sites = {
                codex: "https://chatgpt.com/codex/cloud/settings/analytics",
                openai: "https://platform.openai.com/usage",
                azureopenai: "https://portal.azure.com/#view/Microsoft_Azure_CostManagement/Menu/~/overview",
                claude: "https://platform.claude.com/settings/billing",
                gemini: "https://aistudio.google.com/usage",
                antigravity: "https://antigravity.google/",
                cursor: "https://cursor.com/dashboard",
                opencode: "https://opencode.ai/",
                opencodego: "https://opencode.ai/",
                alibaba: "https://bailian.console.aliyun.com/?tab=model#/efm/usage",
                alibabatokenplan: "https://bailian.console.aliyun.com/?tab=model#/efm/usage",
                zai: "https://open.bigmodel.cn/console/overview",
                abacus: "https://abacus.ai/app",
                copilot: "https://github.com/settings/copilot",
                minimax: "https://platform.minimax.io/subscribe",
                manus: "https://manus.im/app#settings/usage",
                vertexai: "https://console.cloud.google.com/vertex-ai",
                kilo: "https://app.kilo.ai/profile",
                kiro: "https://app.kiro.dev/",
                augment: "https://app.augmentcode.com/",
                factory: "https://app.factory.ai/",
                jetbrains: "https://account.jetbrains.com/licenses",
                kimi: "https://www.kimi.com/code/console",
                kimik2: "https://platform.kimi.com/console",
                moonshot: "https://platform.kimi.com/console",
                amp: "https://ampcode.com/settings",
                t3chat: "https://t3.chat/",
                ollama: "https://ollama.com/settings/keys",
                synthetic: "https://api.synthetic.new/v2/quotas",
                openrouter: "https://openrouter.ai/activity",
                elevenlabs: "https://elevenlabs.io/app/developers/analytics/usage",
                warp: "https://app.warp.dev/usage",
                windsurf: "https://windsurf.com/subscription/usage",
                perplexity: "https://www.perplexity.ai/settings/api",
                mimo: "https://platform.xiaomimimo.com/#/console/balance",
                doubao: "https://console.volcengine.com/ark/region:ark+cn-beijing/overview",
                mistral: "https://console.mistral.ai/usage",
                deepseek: "https://platform.deepseek.com/usage",
                codebuff: "https://codebuff.com/",
                crof: "https://crof.ai/usage_api/",
                venice: "https://venice.ai/settings/api",
                commandcode: "https://commandcode.ai/",
                stepfun: "https://platform.stepfun.com/interface-key",
                bedrock: "https://console.aws.amazon.com/bedrock/",
                grok: "https://grok.com/?_s=usage",
                groq: "https://console.groq.com/dashboard/usage",
                deepgram: "https://console.deepgram.com/projects",
                devin: "https://app.devin.ai/settings/billing"
            };
            return sites[normalizeProviderId(provider)] || "";
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
                devin: "#6769ad",
                demo: "#8b766e"
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

        // Remaining-limit tint: white when full, muted yellow mid, red when low.
        // Stops lean high so the upper range stays pale and ~10% is already pure red.
        function remainingLimitColor(percentLeft) {
            const value = Number(percentLeft);
            if (!Number.isFinite(value)) {
                return Kirigami.Theme.negativeTextColor;
            }
            const t = Math.max(0, Math.min(100, value)) / 100;
            // muted yellow around 55% remaining; pure red by 10%
            const yellowAt = 0.55;
            const redAt = 0.10;
            // soft butter yellow (not pure/neon)
            const yR = 1.0, yG = 0.92, yB = 0.45;
            if (t <= redAt) {
                return Qt.rgba(1, 0, 0, 1);
            }
            if (t >= yellowAt) {
                // white (1,1,1) → muted yellow
                const u = (t - yellowAt) / (1 - yellowAt);
                return Qt.rgba(1, 1 - (1 - yG) * (1 - u), 1 - (1 - yB) * (1 - u), 1);
            }
            // muted yellow → red (1,0,0)
            const u = (t - redAt) / (yellowAt - redAt);
            return Qt.rgba(1, yG * u, yB * u, 1);
        }

        // Pace tint: will the remaining budget last until the window resets?
        // Green = comfortable reserve, yellow = on track but tight, red = the
        // CLI projects the window runs dry before reset. Rows without a pace
        // report fall back to the remaining-limit gradient.
        function paceColor(row, percentLeft) {
            const pace = row && row.pace ? row.pace : null;
            if (!pace || (pace.willLastToReset === null && pace.deltaPercent === null)) {
                return remainingLimitColor(percentLeft);
            }
            if (pace.willLastToReset === false) {
                return Kirigami.Theme.negativeTextColor;
            }
            const delta = Number(pace.deltaPercent);
            // deltaPercent < 0 means budget in reserve versus the expected burn.
            if (Number.isFinite(delta) && delta > -10) {
                return Kirigami.Theme.neutralTextColor;
            }
            return Kirigami.Theme.positiveTextColor;
        }

        function compactBarColor(provider, percentLeft, row) {
            const tint = plasmoid.configuration.compactBarsTint || "provider";
            if (tint === "threshold") {
                return remainingLimitColor(percentLeft);
            }
            if (tint === "pace") {
                return paceColor(row, percentLeft);
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
            // compactBarCatalog writes reliably flush the General group — reassert
            // selection on the same path so KConfig cannot drop it while other
            // keys keep updating across a session.
            if (root.selectionHydrated && (root.selectedEntryIds || []).length > 0) {
                root.persistSelectedEntryIds({ allowEmpty: false });
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
                    color: compactBarColor(entry.provider, percentLeft, row)
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

        function rememberSuccessfulEntries(entries) {
            const remembered = root.lastSuccessfulEntries.slice();
            for (const entry of entries || []) {
                if (!entry || entry.error) {
                    continue;
                }
                const existingIndex = remembered.findIndex(function(candidate) {
                    return candidate && candidate.id === entry.id;
                });
                if (existingIndex >= 0) {
                    remembered[existingIndex] = entry;
                } else {
                    remembered.push(entry);
                }
            }
            root.lastSuccessfulEntries = remembered;
        }

        function lastSuccessfulEntryFor(entry) {
            if (!entry) {
                return null;
            }
            const exact = root.lastSuccessfulEntries.find(function(candidate) {
                return candidate && candidate.id === entry.id;
            });
            if (exact) {
                return exact;
            }

            // Failed CLI entries often omit the account, so their generated id
            // no longer matches the successful one. A provider/source fallback
            // is safe only when it identifies one remembered account.
            const providerMatches = root.lastSuccessfulEntries.filter(function(candidate) {
                return candidate && candidate.provider === entry.provider
                    && (!entry.source || candidate.source === entry.source);
            });
            return providerMatches.length === 1 ? providerMatches[0] : null;
        }

        function retainLastSuccessfulResults(parsed) {
            if (!parsed || parsed.ok === false) {
                return parsed;
            }

            let retainedAny = false;
            let receivedAnySuccess = false;
            const mergedEntries = (parsed.entries || []).map(function(entry) {
                if (!entry || !entry.error) {
                    receivedAnySuccess = receivedAnySuccess || !!entry;
                    const previous = lastSuccessfulEntryFor(entry);
                    const currentRows = entry && entry.rows ? entry.rows : [];
                    const previousRows = previous && previous.rows ? previous.rows : [];
                    // A transient provider response can still be reported as
                    // successful while omitting every rate-limit window. Keep
                    // the known limit rows so limit-based providers do not get
                    // reinterpreted as credit/balance providers.
                    if (entry && currentRows.length === 0 && previousRows.length > 0) {
                        const retained = {};
                        for (const key in entry) {
                            retained[key] = entry[key];
                        }
                        retained.rows = previousRows;
                        retainedAny = true;
                        return retained;
                    }
                    return entry;
                }
                const previous = lastSuccessfulEntryFor(entry);
                if (!previous) {
                    return entry;
                }
                const retained = {};
                for (const key in previous) {
                    retained[key] = previous[key];
                }
                retained.error = entry.error;
                retainedAny = true;
                return retained;
            });

            rememberSuccessfulEntries(mergedEntries);
            if (!retainedAny) {
                return parsed;
            }

            const mergedSnapshot = {};
            for (const key in parsed) {
                mergedSnapshot[key] = parsed[key];
            }
            mergedSnapshot.entries = mergedEntries;
            // When every provider failed, keep the timestamp of the data that
            // is actually on screen instead of claiming it was just updated.
            if (!receivedAnySuccess && root.snapshot.generatedAt) {
                mergedSnapshot.generatedAt = root.snapshot.generatedAt;
            }
            return mergedSnapshot;
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
                root.lastError = parsed.ok === false ? (parsed.error || i18n("CodexBar refresh failed")) : "";
                if (parsed.ok !== false) {
                    root.snapshot = codexBar.retainLastSuccessfulResults(parsed);
                    codexBar.updateCompactBarCatalog(root.snapshot.entries || []);
                }
                if (parsed.cliUpdate && (parsed.cliUpdate.updated || parsed.cliUpdate.error)) {
                    root.cliUpdateInfo = parsed.cliUpdate;
                }
                // Remap remembered selection onto the latest entry ids. Never
                // hard-wipe on id drift (account/index changes across restarts).
                if (parsed.ok !== false) {
                    root.reconcileSelectedEntryIds(root.snapshot.entries || []);
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

    Plasma5Support.DataSource {
        id: siteLauncher
        engine: "executable"
        connectedSources: []
        interval: 0
        onNewData: function(sourceName, data) {
            disconnectSource(sourceName);
        }
    }

    Plasma5Support.DataSource {
        id: selectionStoreRunner
        engine: "executable"
        connectedSources: []
        interval: 0
        onNewData: function(sourceName, data) {
            disconnectSource(sourceName);
            const output = String(data.stdout || data["stdout"] || "").trim();
            if (!output.length) {
                return;
            }
            try {
                const parsed = JSON.parse(output);
                if (parsed && parsed.action === "read-selection") {
                    root.applyStoredSelectionPayload(parsed);
                }
            } catch (error) {
                // Selection-store writes are best-effort.
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

    function openProviderSite(entry) {
        if (!entry) {
            return;
        }
        const url = codexBar.providerSiteUrl(entry.provider, entry);
        if (!url) {
            return;
        }
        const script = codexBar.localPath(Qt.resolvedUrl("../code/codexbar-browser-launcher.mjs"));
        const command = codexBar.quote("node") + " " + codexBar.quote(script)
            + " --url " + codexBar.quote(url)
            + " --source " + codexBar.quote(entry.source || "");
        if (previousSiteLaunchCommand.length > 0) {
            siteLauncher.disconnectSource(previousSiteLaunchCommand);
        }
        previousSiteLaunchCommand = command;
        activeSiteLaunchCommand = command;
        siteLauncher.connectSource(command);
    }

    function resolveVisibleEntries(allEntries, selectedIds) {
        const list = allEntries || [];
        const selected = selectedIds || [];
        if (selected.length === 0) {
            return list;
        }

        const matched = [];
        const used = {};
        function pushUnique(entry) {
            if (!entry || !entry.id || used[entry.id]) {
                return;
            }
            used[entry.id] = true;
            matched.push(entry);
        }

        for (let i = 0; i < selected.length; i += 1) {
            const selectedId = String(selected[i] || "");
            if (!selectedId) {
                continue;
            }
            const exact = list.find(function(entry) { return entry && entry.id === selectedId; });
            if (exact) {
                pushUnique(exact);
                continue;
            }

            const provider = providerKeyFromEntryId(selectedId);
            const rest = selectedId.indexOf(":") === -1 ? "" : selectedId.slice(selectedId.indexOf(":") + 1);
            const candidates = list.filter(function(entry) {
                return entry && String(entry.provider || "") === provider;
            });
            if (candidates.length === 0) {
                continue;
            }
            if (rest) {
                const accountMatch = candidates.find(function(entry) {
                    return entry.account === rest
                        || entry.source === rest
                        || String(entry.id).indexOf(":" + rest) !== -1;
                });
                if (accountMatch) {
                    pushUnique(accountMatch);
                    continue;
                }
            }
            // Provider-only token, or legacy index-based id — show sole/first account.
            pushUnique(candidates[0]);
        }

        // If nothing matched (stale providers only), fall back to all entries
        // rather than rendering an empty dashboard.
        return matched.length > 0 ? matched : list;
    }

    function loadSelectedEntryIds() {
        const fromIds = parseStringListConfig(plasmoid.configuration.selectedEntryIds);
        if (fromIds.length > 0) {
            return fromIds;
        }
        // Older sessions / id-format changes may only have provider names.
        return parseStringListConfig(plasmoid.configuration.selectedProviders);
    }

    function parseStringListConfig(raw) {
        try {
            // Config may already be a JS array when the key is exposed as a list.
            if (Array.isArray(raw)) {
                return raw.map(function(item) { return String(item); }).filter(function(item) {
                    return item.length > 0;
                });
            }
            const parsed = JSON.parse(String(raw || "[]"));
            if (!Array.isArray(parsed)) {
                return [];
            }
            return parsed.map(function(item) { return String(item); }).filter(function(item) {
                return item.length > 0;
            });
        } catch (error) {
            return [];
        }
    }

    function providerKeyFromEntryId(entryId) {
        const value = String(entryId || "");
        const colon = value.indexOf(":");
        return colon === -1 ? value : value.slice(0, colon);
    }

    function appletInstanceKey() {
        // Prefer the Plasma applet id so multi-instance widgets keep separate
        // selections. Fall back to a shared key when id is unavailable.
        try {
            if (Plasmoid && Plasmoid.id !== undefined && Plasmoid.id !== null) {
                return String(Plasmoid.id);
            }
        } catch (error) {
            // Plasmoid.id is not always exposed on every Plasma build.
        }
        try {
            if (plasmoid && plasmoid.id !== undefined && plasmoid.id !== null) {
                return String(plasmoid.id);
            }
        } catch (error) {
            // ignore
        }
        return "default";
    }

    /**
     * Persist switcher selection.
     * options.allowEmpty — when false, refuse to overwrite a non-empty saved
     *   selection with []. Blocks plasmashell teardown / init races from
     *   wiping the last choice.
     * options.force — write even when the serialized value matches config.
     */
    function persistSelectedEntryIds(options) {
        if (!selectionHydrated && !(options && options.force)) {
            return;
        }
        const allowEmpty = !!(options && options.allowEmpty);
        const force = !!(options && options.force);
        const ids = selectedEntryIds || [];
        const serializedIds = JSON.stringify(ids);

        if (ids.length === 0 && !allowEmpty) {
            const existingIds = String(plasmoid.configuration.selectedEntryIds || "[]");
            if (existingIds !== "[]" && existingIds !== "") {
                return;
            }
            const existingProviders = String(plasmoid.configuration.selectedProviders || "[]");
            if (existingProviders !== "[]" && existingProviders !== "") {
                return;
            }
        }

        // Write via both accessors; some Plasma builds only dirty one path.
        if (force || serializedIds !== String(plasmoid.configuration.selectedEntryIds || "[]")) {
            plasmoid.configuration.selectedEntryIds = serializedIds;
            try { Plasmoid.configuration.selectedEntryIds = serializedIds; } catch (error) {}
        }

        // Provider names are a durable fallback when entry ids change shape
        // (account appears/disappears, source changes, id format upgrades).
        const providers = [];
        const seen = {};
        for (let i = 0; i < ids.length; i += 1) {
            const provider = providerKeyFromEntryId(ids[i]);
            if (!provider || seen[provider]) {
                continue;
            }
            seen[provider] = true;
            providers.push(provider);
        }
        const serializedProviders = JSON.stringify(providers);
        if (force || serializedProviders !== String(plasmoid.configuration.selectedProviders || "[]")) {
            plasmoid.configuration.selectedProviders = serializedProviders;
            try { Plasmoid.configuration.selectedProviders = serializedProviders; } catch (error) {}
        }

        // Side-file backup: survives config-dialog Apply wiping undeclared keys
        // and KConfig not flushing applet string keys before a hard restart.
        writeSelectedEntryIdsToStore(ids, providers);
    }

    function selectionStoreScriptPath() {
        return codexBar.localPath(Qt.resolvedUrl("../code/codexbar-provider-sync.mjs"));
    }

    function writeSelectedEntryIdsToStore(ids, providers) {
        const payload = JSON.stringify({
            entryIds: ids || [],
            providers: providers || []
        });
        const command = codexBar.quote(selectionStoreScriptPath())
            + " --action write-selection"
            + " --instance " + codexBar.quote(appletInstanceKey())
            + " --selection " + codexBar.quote(payload);
        if (previousSelectionStoreCommand.length > 0) {
            selectionStoreRunner.disconnectSource(previousSelectionStoreCommand);
        }
        previousSelectionStoreCommand = command;
        selectionStoreRunner.connectSource(command);
    }

    function loadSelectedEntryIdsFromStore() {
        const command = codexBar.quote(selectionStoreScriptPath())
            + " --action read-selection"
            + " --instance " + codexBar.quote(appletInstanceKey());
        if (previousSelectionStoreCommand.length > 0) {
            selectionStoreRunner.disconnectSource(previousSelectionStoreCommand);
        }
        previousSelectionStoreCommand = command;
        selectionStoreRunner.connectSource(command);
    }

    function applyStoredSelectionPayload(payload) {
        if (!payload || payload.ok === false) {
            return;
        }
        // Config already had a selection — keep it as the source of truth.
        if ((selectedEntryIds || []).length > 0) {
            return;
        }
        let restored = [];
        if (Array.isArray(payload.entryIds) && payload.entryIds.length > 0) {
            restored = payload.entryIds.map(function(item) { return String(item); }).filter(function(item) {
                return item.length > 0;
            });
        } else if (Array.isArray(payload.providers) && payload.providers.length > 0) {
            restored = payload.providers.map(function(item) { return String(item); }).filter(function(item) {
                return item.length > 0;
            });
        }
        if (restored.length === 0) {
            return;
        }
        if (!multiProviderSelectionEnabled && restored.length > 1) {
            restored = restored.slice(0, 1);
        }
        selectedEntryIds = restored;
        persistSelectedEntryIds({ allowEmpty: false, force: true });
    }

    /**
     * Map remembered selection onto the current entry list without dropping
     * selections just because the id string changed across a Plasma restart.
     */
    function reconcileSelectedEntryIds(entries) {
        const previous = selectedEntryIds || [];
        if (previous.length === 0) {
            return;
        }
        if (!entries || entries.length === 0) {
            // Keep the remembered selection while data is empty/unavailable.
            return;
        }

        const availableIds = {};
        const byProvider = {};
        for (let i = 0; i < entries.length; i += 1) {
            const entry = entries[i];
            if (!entry || !entry.id) {
                continue;
            }
            availableIds[entry.id] = entry;
            const provider = String(entry.provider || providerKeyFromEntryId(entry.id));
            if (!byProvider[provider]) {
                byProvider[provider] = [];
            }
            byProvider[provider].push(entry);
        }

        const next = [];
        const used = {};
        function pushUnique(entry) {
            if (!entry || !entry.id || used[entry.id]) {
                return;
            }
            used[entry.id] = true;
            next.push(entry.id);
        }

        for (let i = 0; i < previous.length; i += 1) {
            const selectedId = String(previous[i] || "");
            if (!selectedId) {
                continue;
            }
            if (availableIds[selectedId]) {
                pushUnique(availableIds[selectedId]);
                continue;
            }

            const provider = providerKeyFromEntryId(selectedId);
            const rest = selectedId.indexOf(":") === -1 ? "" : selectedId.slice(selectedId.indexOf(":") + 1);
            const candidates = byProvider[provider] || [];
            if (candidates.length === 0) {
                // Provider not in this snapshot — keep the token for a later
                // refresh instead of permanently forgetting the choice.
                if (!used[selectedId]) {
                    used[selectedId] = true;
                    next.push(selectedId);
                }
                continue;
            }

            let match = null;
            if (rest) {
                match = candidates.find(function(entry) {
                    return entry.account === rest
                        || entry.source === rest
                        || String(entry.id) === selectedId
                        || String(entry.id).indexOf(":" + rest) !== -1;
                }) || null;
            }
            if (!match && candidates.length === 1) {
                match = candidates[0];
            }
            if (!match && provider === selectedId && candidates.length > 0) {
                // Provider-only token from selectedProviders fallback.
                match = candidates[0];
            }
            if (match) {
                pushUnique(match);
            } else {
                // Provider is present but the old account/source token no longer
                // matches — keep the first live account instead of a dead id.
                pushUnique(candidates[0]);
            }
        }

        if (!multiProviderSelectionEnabled && next.length > 1) {
            next.length = 1;
        }

        if (JSON.stringify(next) !== JSON.stringify(previous)) {
            selectedEntryIds = next;
            // Remapped ids should stick across restarts. Do not allowEmpty —
            // reconcile should not invent a total clear.
            persistSelectedEntryIds({ allowEmpty: false });
        } else if (selectionHydrated && previous.length > 0) {
            // Same tokens, but re-assert so KConfig keeps a dirty write after
            // long sessions / refresh-only updates.
            persistSelectedEntryIds({ allowEmpty: false });
        }
    }

    function toggleEntrySelection(entryId) {
        if (!multiProviderSelectionEnabled) {
            selectedEntryIds = selectedEntryIds.length === 1 && selectedEntryIds[0] === entryId
                ? []
                : [entryId];
            // User-driven: empty means "show all" and must be remembered.
            persistSelectedEntryIds({ allowEmpty: true });
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
        persistSelectedEntryIds({ allowEmpty: true });
    }

    function entryJsonForModel(entry) {
        try {
            return JSON.stringify(entry || {});
        } catch (error) {
            return "{}";
        }
    }

    function visibleEntryIdsSignature(list) {
        const ids = [];
        for (let index = 0; index < list.length; index += 1) {
            ids.push(String(list[index] && list[index].id || ""));
        }
        return ids.join("\n");
    }

    function syncProviderEntryModel() {
        if (root.providerReorderActive) {
            return;
        }
        const list = root.visibleEntries || [];
        let sameOrder = providerEntryModel.count === list.length;
        if (sameOrder) {
            for (let index = 0; index < list.length; index += 1) {
                if (String(providerEntryModel.get(index).entryId || "") !== String(list[index].id || "")) {
                    sameOrder = false;
                    break;
                }
            }
        }
        if (sameOrder) {
            for (let index = 0; index < list.length; index += 1) {
                const entry = list[index];
                providerEntryModel.set(index, {
                    entryId: String(entry.id || ""),
                    provider: String(entry.provider || ""),
                    entryJson: entryJsonForModel(entry)
                });
            }
            return;
        }
        providerEntryModel.clear();
        for (let index = 0; index < list.length; index += 1) {
            const entry = list[index];
            providerEntryModel.append({
                entryId: String(entry.id || ""),
                provider: String(entry.provider || ""),
                entryJson: entryJsonForModel(entry)
            });
        }
    }

    // Apply a full provider order from the top chip switcher and persist it.
    function applyProviderOrder(orderedEntries) {
        if (!orderedEntries || orderedEntries.length === 0) {
            return;
        }
        const nextSnapshot = {};
        for (const key in root.snapshot) {
            nextSnapshot[key] = root.snapshot[key];
        }
        nextSnapshot.entries = orderedEntries.slice();
        const wasReordering = root.providerReorderActive;
        root.providerReorderActive = true;
        root.snapshot = nextSnapshot;
        root.providerReorderActive = wasReordering;
        persistProviderConfigsOrder(nextSnapshot.entries);
        Qt.callLater(syncProviderEntryModel);
    }

    function persistProviderConfigsOrder(orderedEntries) {
        const configs = codexBar.parseProviderConfigs();
        if (!Array.isArray(configs) || configs.length === 0) {
            // Automatic-discovery mode has no ordered config list to rewrite.
            return;
        }

        const used = [];
        for (let index = 0; index < configs.length; index += 1) {
            used.push(false);
        }

        const reordered = [];
        for (let entryIndex = 0; entryIndex < orderedEntries.length; entryIndex += 1) {
            const entry = orderedEntries[entryIndex];
            if (!entry) {
                continue;
            }
            const entryProvider = codexBar.normalizeProviderId(entry.provider);
            const entryAccount = String(entry.account || "");
            let match = -1;

            for (let index = 0; index < configs.length; index += 1) {
                if (used[index]) {
                    continue;
                }
                if (codexBar.normalizeProviderId(configs[index].provider) !== entryProvider) {
                    continue;
                }
                const configAccount = String(configs[index].account || "");
                if (configAccount.length > 0 && entryAccount.length > 0 && configAccount === entryAccount) {
                    match = index;
                    break;
                }
            }
            if (match < 0) {
                for (let index = 0; index < configs.length; index += 1) {
                    if (used[index]) {
                        continue;
                    }
                    if (codexBar.normalizeProviderId(configs[index].provider) === entryProvider) {
                        match = index;
                        break;
                    }
                }
            }
            if (match >= 0) {
                used[match] = true;
                reordered.push(configs[match]);
            }
        }
        for (let index = 0; index < configs.length; index += 1) {
            if (!used[index]) {
                reordered.push(configs[index]);
            }
        }

        const serialized = JSON.stringify(reordered);
        if (serialized !== String(plasmoid.configuration.providerConfigs || "")) {
            plasmoid.configuration.providerConfigs = serialized;
        }
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

        // Panel popups are sized by PlasmaCore.AppletPopup from these Layout
        // hints. Leave maximumWidth/Height unset so the popup stays freely
        // resizable (edges drag; size remembered as popupWidth/popupHeight).
        // A hard maximumHeight previously blocked growing past ~32 grid units.
        Layout.minimumWidth: Kirigami.Units.gridUnit * 18
        Layout.minimumHeight: Kirigami.Units.gridUnit * 12
        Layout.preferredWidth: Kirigami.Units.gridUnit * 24
        Layout.preferredHeight: Kirigami.Units.gridUnit * 28
        Layout.fillWidth: true
        Layout.fillHeight: true
        collapseMarginsHint: true

        contentItem: ColumnLayout {
            id: expandedContent
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.smallSpacing
                // Prefer !== false so missing config (pre-upgrade) still shows the bar.
                visible: plasmoid.configuration.showTopBar !== false

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
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                Layout.topMargin: Kirigami.Units.smallSpacing
                entries: root.entries
                selectedEntryIds: root.effectiveSelectedEntryIds
                style: {
                    const value = String(plasmoid.configuration.providerSwitcherStyle || "tabs");
                    if (value === "chips" || value === "none" || value === "tabs") {
                        return value;
                    }
                    return "tabs";
                }
                reorderEnabled: root.entries.length > 1
                dragContainer: representation
                colorForProvider: function(provider) { return codexBar.color(provider); }
                onEntrySelected: function(entryId) {
                    root.toggleEntrySelection(entryId);
                }
                onOrderCommitted: function(orderedEntries) {
                    root.applyProviderOrder(orderedEntries);
                }
                onReorderActiveChanged: {
                    root.providerReorderActive = reorderActive;
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

            ListView {
                id: providerList

                // Attached ScrollBar paints over the view; reserve its width so
                // right-edge labels (status, %, source) are not covered.
                // Prefer !== false so missing config (pre-upgrade) still shows the bar.
                readonly property bool scrollBarEnabled: plasmoid.configuration.showScrollbar !== false
                readonly property bool scrollBarNeeded: scrollBarEnabled && contentHeight > height + 1
                readonly property real scrollBarInset: scrollBarNeeded
                    ? Math.max(verticalScrollBar.implicitWidth, Kirigami.Units.gridUnit * 0.75)
                      + Kirigami.Units.smallSpacing
                    : 0

                Layout.fillWidth: true
                // Fill whatever height the user gave the popup; do not cap the
                // list — that left empty chrome and blocked taller resizes.
                Layout.fillHeight: true
                Layout.minimumHeight: providerEntryModel.count > 0 ? Kirigami.Units.gridUnit : 0
                visible: providerEntryModel.count > 0
                clip: true
                spacing: Kirigami.Units.smallSpacing
                boundsBehavior: Flickable.StopAtBounds
                cacheBuffer: Kirigami.Units.gridUnit * 8

                QtControls.ScrollBar.vertical: QtControls.ScrollBar {
                    id: verticalScrollBar
                    policy: providerList.scrollBarNeeded
                        ? QtControls.ScrollBar.AsNeeded
                        : QtControls.ScrollBar.AlwaysOff
                }

                model: providerEntryModel

                delegate: ProviderCard {
                    id: providerCard

                    required property string entryId
                    required property string provider
                    required property string entryJson

                    readonly property var entryData: {
                        try {
                            return JSON.parse(entryJson || "{}");
                        } catch (error) {
                            return null;
                        }
                    }

                    width: Math.max(
                        0,
                        providerList.width
                            - Kirigami.Units.smallSpacing * 2
                            - providerList.scrollBarInset
                    )
                    x: Kirigami.Units.smallSpacing

                    entry: providerCard.entryData
                    providerName: codexBar.providerName(
                        providerCard.entryData ? providerCard.entryData.provider : providerCard.provider
                    )
                    providerSiteUrl: providerCard.entryData
                        ? codexBar.providerSiteUrl(providerCard.entryData.provider, providerCard.entryData)
                        : ""
                    accentColor: codexBar.color(
                        providerCard.entryData ? providerCard.entryData.provider : providerCard.provider
                    )
                    showCredits: plasmoid.configuration.showCredits
                    showHistory: plasmoid.configuration.showHistory
                    onSiteRequested: root.openProviderSite(providerCard.entryData)
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
