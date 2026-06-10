import QtQuick
import QtQuick.Controls as QtControls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    property string cfg_cliPath: cliPath.text
    property string cfg_provider: "codex"
    property string cfg_source: "auto"
    property string cfg_providerConfigs: ""
    property string cfg_account: ""
    property int cfg_accountIndex: 0
    property bool cfg_allAccounts: false
    property alias cfg_includeStatus: includeStatus.checked
    property alias cfg_includeCost: includeCost.checked
    property alias cfg_showCredits: showCredits.checked
    property alias cfg_showHistory: showHistory.checked
    property alias cfg_anonymizeEmail: anonymizeEmail.checked
    property int cfg_refreshIntervalSeconds: refreshInterval.value
    property int cfg_requestTimeoutSeconds: requestTimeout.value
    property alias cfg_compactMetric: compactMetric.currentValue
    property alias cfg_compactDisplay: compactDisplay.currentValue

    readonly property var sourceLabels: ({
        auto: i18n("Auto"),
        cli: i18n("CLI/local"),
        oauth: i18n("OAuth"),
        api: i18n("API"),
        web: i18n("Web"),
        native: i18n("Native")
    })
    readonly property var sourceNotes: ({
        auto: i18n("Provider default"),
        cli: i18n("Local agent files"),
        oauth: i18n("Signed-in account"),
        api: i18n("API credentials"),
        web: i18n("Browser/web session"),
        native: i18n("Plasmoid fetcher")
    })
    readonly property var providerCatalog: [
        { id: "codex", name: "Codex", sources: ["auto", "cli", "oauth", "web"], linuxDefault: "cli" },
        { id: "claude", name: "Claude", sources: ["auto", "cli", "oauth", "api", "web"], linuxDefault: "cli" },
        { id: "cursor", name: "Cursor", sources: ["auto", "native", "cli"], linuxDefault: "native" },
        { id: "gemini", name: "Gemini", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "copilot", name: "Copilot", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "openai", name: "OpenAI API", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "azure-openai", name: "Azure OpenAI", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "antigravity", name: "Antigravity", sources: ["auto", "native", "cli", "oauth"], linuxDefault: "native" },
        { id: "augment", name: "Augment", sources: ["auto", "cli"], linuxDefault: "cli" },
        { id: "factory", name: "Factory", sources: ["auto", "cli"], linuxDefault: "cli" },
        { id: "jetbrains", name: "JetBrains", sources: ["auto", "cli"], linuxDefault: "cli" },
        { id: "kilo", name: "Kilo", sources: ["auto", "api", "cli"], linuxDefault: "api" },
        { id: "kiro", name: "Kiro", sources: ["auto", "cli"], linuxDefault: "cli" },
        { id: "grok", name: "Grok", sources: ["auto", "cli", "web"], linuxDefault: "cli" },
        { id: "ollama", name: "Ollama", sources: ["auto", "api", "web"], linuxDefault: "api" },
        { id: "minimax", name: "MiniMax", sources: ["auto", "api", "web"], linuxDefault: "api" },
        { id: "alibaba-coding-plan", name: "Alibaba Coding", sources: ["auto", "api", "web"], linuxDefault: "api" },
        { id: "bedrock", name: "Bedrock", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "deepgram", name: "Deepgram", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "deepseek", name: "DeepSeek", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "doubao", name: "Doubao", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "groqcloud", name: "GroqCloud", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "kimik2", name: "Kimi K2", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "llmproxy", name: "LLM Proxy", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "moonshot", name: "Moonshot", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "openrouter", name: "OpenRouter", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "synthetic", name: "Synthetic", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "venice", name: "Venice", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "zai", name: "z.ai", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "abacusai", name: "Abacus", sources: ["auto", "web"], linuxDefault: "web" },
        { id: "amp", name: "Amp", sources: ["auto", "web"], linuxDefault: "web" },
        { id: "commandcode", name: "Command Code", sources: ["auto", "web"], linuxDefault: "web" },
        { id: "kimi", name: "Kimi", sources: ["auto", "web"], linuxDefault: "web" },
        { id: "manus", name: "Manus", sources: ["auto", "web"], linuxDefault: "web" },
        { id: "mimo", name: "MiMo", sources: ["auto", "web"], linuxDefault: "web" },
        { id: "mistral", name: "Mistral", sources: ["auto", "web"], linuxDefault: "web" },
        { id: "opencode", name: "OpenCode", sources: ["auto", "native", "web"], linuxDefault: "native" },
        { id: "opencodego", name: "OpenCode Go", sources: ["auto", "native", "web"], linuxDefault: "native" },
        { id: "perplexity", name: "Perplexity", sources: ["auto", "web"], linuxDefault: "web" },
        { id: "stepfun", name: "StepFun", sources: ["auto", "web"], linuxDefault: "web" },
        { id: "t3chat", name: "T3 Chat", sources: ["auto", "web"], linuxDefault: "web" },
        { id: "vertexai", name: "Vertex AI", sources: ["auto", "oauth"], linuxDefault: "oauth" },
        { id: "windsurf", name: "Windsurf", sources: ["auto", "cli", "web"], linuxDefault: "cli" }
    ]

    Component.onCompleted: loadProviders()

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.largeSpacing

        Kirigami.FormLayout {
            Layout.fillWidth: true

            QtControls.TextField {
                id: cliPath
                Kirigami.FormData.label: i18n("CLI executable:")
                placeholderText: "codexbar"
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Heading {
                Layout.fillWidth: true
                level: 3
                text: i18n("Providers")
            }

            Repeater {
                model: providerModel

                delegate: Kirigami.AbstractCard {
                    id: providerDelegate

                    required property int index
                    required property string provider
                    required property string source
                    required property bool enabled
                    required property string account
                    required property int accountIndex
                    required property bool allAccounts

                    Layout.fillWidth: true
                    padding: Kirigami.Units.smallSpacing

                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing

                        RowLayout {
                            Layout.fillWidth: true

                            QtControls.CheckBox {
                                checked: providerDelegate.enabled
                                onToggled: page.setProviderProperty(providerDelegate.index, "enabled", checked)
                            }

                            QtControls.ComboBox {
                                Layout.fillWidth: true
                                textRole: "name"
                                valueRole: "id"
                                model: page.providerCatalog
                                currentIndex: page.providerIndex(providerDelegate.provider)
                                onActivated: function(row) {
                                    const selected = page.providerCatalog[row];
                                    page.setProviderProperty(providerDelegate.index, "provider", selected.id);
                                    if (selected.sources.indexOf(providerDelegate.source) === -1) {
                                        page.setProviderProperty(providerDelegate.index, "source", selected.linuxDefault);
                                    }
                                }
                            }

                            QtControls.ToolButton {
                                icon.name: "list-remove"
                                text: i18n("Remove")
                                display: QtControls.AbstractButton.IconOnly
                                onClicked: {
                                    providerModel.remove(providerDelegate.index);
                                    page.syncConfig();
                                }
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true

                            QtControls.ComboBox {
                                id: sourceCombo
                                Layout.fillWidth: true
                                textRole: "text"
                                valueRole: "value"
                                model: page.sourceModel(providerDelegate.provider)
                                currentIndex: page.sourceIndex(providerDelegate.provider, providerDelegate.source)
                                onActivated: function(row) {
                                    page.setProviderProperty(providerDelegate.index, "source", sourceCombo.model[row].value);
                                }
                            }

                            QtControls.Label {
                                Layout.fillWidth: true
                                text: page.sourceNotes[providerDelegate.source] || ""
                                color: Kirigami.Theme.disabledTextColor
                                elide: Text.ElideRight
                            }
                        }

                        QtControls.Button {
                            id: accountToggle
                            Layout.fillWidth: true
                            checkable: true
                            checked: providerDelegate.account.length > 0 || providerDelegate.accountIndex > 0 || providerDelegate.allAccounts
                            text: checked ? i18n("Account filter enabled") : i18n("Account filter")
                            icon.name: "user-identity"
                            onToggled: {
                                if (!checked) {
                                    page.setProviderProperty(providerDelegate.index, "account", "");
                                    page.setProviderProperty(providerDelegate.index, "accountIndex", 0);
                                    page.setProviderProperty(providerDelegate.index, "allAccounts", false);
                                }
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            visible: accountToggle.checked

                            QtControls.TextField {
                                Layout.fillWidth: true
                                text: providerDelegate.account
                                placeholderText: i18n("Account name")
                                enabled: !providerDelegate.allAccounts
                                onEditingFinished: page.setProviderProperty(providerDelegate.index, "account", text)
                            }

                            QtControls.SpinBox {
                                from: 0
                                to: 99
                                value: providerDelegate.accountIndex
                                editable: true
                                enabled: !providerDelegate.allAccounts && providerDelegate.account.length === 0
                                textFromValue: function(value) {
                                    return value === 0 ? i18n("Any") : i18n("#%1", value);
                                }
                                valueFromText: function(text) {
                                    const parsed = parseInt(text, 10);
                                    return Number.isFinite(parsed) ? parsed : 0;
                                }
                                onValueModified: page.setProviderProperty(providerDelegate.index, "accountIndex", value)
                            }

                            QtControls.CheckBox {
                                text: i18n("All accounts")
                                checked: providerDelegate.allAccounts
                                onToggled: page.setProviderProperty(providerDelegate.index, "allAccounts", checked)
                            }
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true

                QtControls.Button {
                    icon.name: "list-add"
                    text: i18n("Add Provider")
                    onClicked: {
                        const provider = page.firstMissingProvider();
                        providerModel.append({
                            provider: provider.id,
                            source: provider.linuxDefault,
                            enabled: true,
                            account: "",
                            accountIndex: 0,
                            allAccounts: false
                        });
                        page.syncConfig();
                    }
                }

                QtControls.Label {
                    Layout.fillWidth: true
                    text: i18n("Sources are per provider; Linux defaults avoid macOS-only web probes.")
                    color: Kirigami.Theme.disabledTextColor
                    wrapMode: Text.Wrap
                }
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            QtControls.CheckBox {
                id: includeStatus
                Kirigami.FormData.label: i18n("Extras:")
                text: i18n("Provider status")
            }

            QtControls.CheckBox {
                id: includeCost
                text: i18n("Local token costs")
            }

            QtControls.CheckBox {
                id: showCredits
                text: i18n("Credits")
            }

            QtControls.CheckBox {
                id: showHistory
                text: i18n("History chart")
            }

            QtControls.CheckBox {
                id: anonymizeEmail
                text: i18n("Anonymize emails")
            }

            QtControls.SpinBox {
                id: refreshInterval
                Kirigami.FormData.label: i18n("Refresh:")
                from: 30
                to: 86400
                stepSize: 30
                editable: true
                textFromValue: function(value) {
                    if (value < 90) {
                        return i18np("%1 second", "%1 seconds", value);
                    }
                    return i18np("%1 minute", "%1 minutes", Math.round(value / 60));
                }
                valueFromText: function(text) {
                    const parsed = parseInt(text, 10);
                    return Number.isFinite(parsed) ? parsed : 300;
                }
            }

            QtControls.SpinBox {
                id: requestTimeout
                Kirigami.FormData.label: i18n("Timeout:")
                from: 5
                to: 300
                stepSize: 5
                editable: true
                textFromValue: function(value) {
                    return i18np("%1 second", "%1 seconds", value);
                }
                valueFromText: function(text) {
                    const parsed = parseInt(text, 10);
                    return Number.isFinite(parsed) ? parsed : 45;
                }
            }

            QtControls.ComboBox {
                id: compactMetric
                Kirigami.FormData.label: i18n("Compact metric:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: i18n("Lowest remaining limit"), value: "lowest" },
                    { text: i18n("Session remaining"), value: "session" },
                    { text: i18n("Weekly remaining"), value: "weekly" },
                    { text: i18n("Credits remaining"), value: "credits" },
                    { text: i18n("Today cost"), value: "todayCost" }
                ]
            }

            QtControls.ComboBox {
                id: compactDisplay
                Kirigami.FormData.label: i18n("Tray display:")
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: i18n("Provider icon"), value: "icon" },
                    { text: i18n("Usage bars"), value: "bars" }
                ]
            }
        }
    }

    ListModel {
        id: providerModel
    }

    function loadProviders() {
        providerModel.clear();
        const parsed = parseProviderConfigs(plasmoid.configuration.providerConfigs);
        for (let index = 0; index < parsed.length; index += 1) {
            providerModel.append(parsed[index]);
        }
        if (providerModel.count === 0) {
            const fallback = parseProviderConfigs("");
            for (let index = 0; index < fallback.length; index += 1) {
                providerModel.append(fallback[index]);
            }
        }
        syncConfig();
    }

    function setProviderProperty(index, key, value) {
        providerModel.setProperty(index, key, value);
        syncConfig();
    }

    function syncConfig() {
        cfg_providerConfigs = serializeProviders();
        cfg_provider = enabledProviders().join(",");
    }

    function parseProviderConfigs(raw) {
        let items = [];
        try {
            items = JSON.parse(String(raw || ""));
        } catch (error) {
            items = [];
        }
        if (!Array.isArray(items) || items.length === 0) {
            const legacyProvider = String(plasmoid.configuration.provider || "codex");
            const legacySource = String(plasmoid.configuration.source || "auto");
            const providers = legacyProvider === "all"
                ? ["codex", "gemini"]
                : legacyProvider.split(",").map(function(item) { return item.trim(); }).filter(function(item) { return item.length > 0; });
            items = providers.map(function(provider) {
                return { provider: provider, source: legacySource, enabled: true };
            });
        }
        return items.map(function(item) {
            const provider = catalogFor(item.provider);
            const source = provider.sources.indexOf(item.source) === -1 ? provider.linuxDefault : item.source;
            return {
                provider: provider.id,
                source: source,
                enabled: item.enabled !== false,
                account: String(item.account || ""),
                accountIndex: Math.max(0, Number(item.accountIndex || 0)),
                allAccounts: item.allAccounts === true
            };
        });
    }

    function serializeProviders() {
        const items = [];
        for (let index = 0; index < providerModel.count; index += 1) {
            const item = providerModel.get(index);
            items.push({
                provider: item.provider,
                source: item.source,
                enabled: item.enabled,
                account: item.account,
                accountIndex: item.accountIndex,
                allAccounts: item.allAccounts
            });
        }
        return JSON.stringify(items);
    }

    function enabledProviders() {
        const providers = [];
        for (let index = 0; index < providerModel.count; index += 1) {
            const item = providerModel.get(index);
            if (item.enabled) {
                providers.push(item.provider);
            }
        }
        return providers.length > 0 ? providers : ["codex"];
    }

    function catalogFor(providerId) {
        const normalized = String(providerId || "codex").toLowerCase();
        for (let index = 0; index < providerCatalog.length; index += 1) {
            const provider = providerCatalog[index];
            if (provider.id === normalized) {
                return provider;
            }
        }
        return providerCatalog[0];
    }

    function providerIndex(providerId) {
        const normalized = catalogFor(providerId).id;
        for (let index = 0; index < providerCatalog.length; index += 1) {
            if (providerCatalog[index].id === normalized) {
                return index;
            }
        }
        return 0;
    }

    function sourceModel(providerId) {
        const provider = catalogFor(providerId);
        return provider.sources.map(function(source) {
            return { text: sourceLabels[source] || source, value: source };
        });
    }

    function sourceIndex(providerId, source) {
        const model = sourceModel(providerId);
        for (let index = 0; index < model.length; index += 1) {
            if (model[index].value === source) {
                return index;
            }
        }
        return 0;
    }

    function firstMissingProvider() {
        const used = {};
        for (let index = 0; index < providerModel.count; index += 1) {
            used[providerModel.get(index).provider] = true;
        }
        for (let index = 0; index < providerCatalog.length; index += 1) {
            if (!used[providerCatalog[index].id]) {
                return providerCatalog[index];
            }
        }
        return providerCatalog[0];
    }
}
