import QtQuick
import QtQuick.Controls as QtControls
import QtQuick.Layouts
import QtQuick.Dialogs as QtDialogs
import org.kde.kirigami as Kirigami
import org.kde.plasma.plasma5support as Plasma5Support
import QtQml.Models

Kirigami.ScrollablePage {
    id: page

    property string cfg_provider: "codex"
    property string cfg_source: "auto"
    property string cfg_providerConfigs: ""
    property alias cfg_allowMultiProviderSelection: allowMultiProviderSelection.checked
    property alias cfg_providerSwitcherStyle: providerSwitcherStyle.currentValue
    // Hidden: keep switcher selection across Configure → Apply. Without cfg_
    // bindings, Plasma rewrites the General group and drops these keys.
    property string cfg_selectedEntryIds: "[]"
    property string cfg_selectedProviders: "[]"
    property string cfg_compactBarCatalog: "{}"
    property string cfg_account: ""
    property int cfg_accountIndex: 0
    property bool cfg_allAccounts: false
    property alias cfg_syncProviders: syncProviders.checked
    property string antigravityAuthStatus: ""
    property bool antigravityAuthBusy: false
    property bool providerSyncApplying: false
    property string providerSyncStatus: ""

    readonly property var sourceLabels: ({
        auto: i18n("Auto"),
        cli: i18n("CLI/local"),
        oauth: i18n("OAuth"),
        api: i18n("API"),
        web: i18n("Web"),
        native: i18n("Linux Helper"),
        "native-auth": i18n("Native Auth")
    })
    readonly property var sourceNotes: ({
        auto: i18n("Provider default"),
        cli: i18n("Local agent files"),
        oauth: i18n("Signed-in account"),
        api: i18n("API credentials"),
        web: i18n("Browser/web session"),
        native: i18n("Bundled Linux helper"),
        "native-auth": i18n("Browser Google OAuth (codexbar-plasmoid login)")
    })
    readonly property var providerCatalog: [
        { id: "codex", name: "Codex", sources: ["auto", "cli", "oauth", "web"], linuxDefault: "cli" },
        { id: "claude", name: "Claude", sources: ["auto", "cli", "oauth", "api", "web"], linuxDefault: "cli" },
        { id: "cursor", name: "Cursor", sources: ["auto", "native", "cli"], linuxDefault: "native" },
        { id: "gemini", name: "Gemini", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "copilot", name: "Copilot", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "openai", name: "OpenAI API", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "azure-openai", name: "Azure OpenAI", sources: ["auto", "api"], linuxDefault: "api" },
        { id: "antigravity", name: "Antigravity", sources: ["auto", "native", "native-auth", "cli", "oauth"], linuxDefault: "native" },
        { id: "augment", name: "Augment", sources: ["auto", "cli"], linuxDefault: "cli" },
        { id: "factory", name: "Factory", sources: ["auto", "cli"], linuxDefault: "cli" },
        { id: "jetbrains", name: "JetBrains", sources: ["auto", "cli"], linuxDefault: "cli" },
        { id: "kilo", name: "Kilo", sources: ["auto", "api", "cli"], linuxDefault: "api" },
        { id: "kiro", name: "Kiro", sources: ["auto", "cli"], linuxDefault: "cli" },
        { id: "grok", name: "Grok", sources: ["auto", "native", "cli", "web"], linuxDefault: "native" },
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
        { id: "windsurf", name: "Windsurf", sources: ["auto", "cli", "web"], linuxDefault: "cli" },
        { id: "devin", name: "Devin", sources: ["auto", "native", "web"], linuxDefault: "native" }
    ]

    Component.onCompleted: {
        loadProviders();
        if (syncProviders.checked) {
            Qt.callLater(readSharedProviders);
        }
    }

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.smallSpacing

            QtControls.CheckBox {
                id: allowMultiProviderSelection
                Layout.fillWidth: true
                text: i18n("Allow selecting multiple providers in the widget")
            }


            QtControls.ComboBox {
                id: providerSwitcherStyle
                Layout.fillWidth: true
                textRole: "text"
                valueRole: "value"
                model: [
                    { text: i18n("Tabs (compact pills)"), value: "tabs" },
                    { text: i18n("Cards (usage bar + account)"), value: "chips" },
                    { text: i18n("None (hide switcher)"), value: "none" }
                ]
            }

            QtControls.Label {
                Layout.fillWidth: true
                text: i18n("Provider switcher above the cards. None hides it completely (selection still works from the tray / multi-select if enabled).")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }

            QtControls.CheckBox {
                id: syncProviders
                Layout.fillWidth: true
                text: i18n("Sync providers between widgets")
                onClicked: {
                    if (checked) {
                        page.readSharedProviders();
                    } else {
                        page.providerSyncStatus = "";
                    }
                }
            }

            QtControls.Label {
                Layout.fillWidth: true
                visible: syncProviders.checked
                text: page.providerSyncStatus.length > 0
                    ? page.providerSyncStatus
                    : i18n("Provider order, sources, accounts, and enabled state are shared. Tray appearance stays local.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.Wrap
            }

            ListView {
                id: providersListView
                Layout.fillWidth: true
                implicitHeight: contentHeight
                interactive: false
                spacing: Kirigami.Units.smallSpacing

                displaced: Transition {
                    NumberAnimation {
                        properties: "x,y"
                        easing.type: Easing.OutQuad
                        duration: 150
                    }
                }

                model: DelegateModel {
                    id: visualModel
                    model: providerModel
                    delegate: DropArea {
                        id: delegateRoot

                        required property int index
                        required property string provider
                        required property string source
                        required property bool enabled
                        required property string account
                        required property int accountIndex
                        required property bool allAccounts
                        required property bool showInCompactAll
                        required property int compactBarLimit
                        required property string compactBarIds
                        required property string compactColor
                        required property string apiKey
                        required property bool includeCost

                        property int visualIndex: DelegateModel.itemsIndex

                        width: providersListView.width
                        height: providerDelegate.implicitHeight

                        onEntered: (drag) => {
                            if (drag.source && drag.source.visualIndex !== undefined) {
                                const fromIndex = drag.source.visualIndex;
                                const toIndex = delegateRoot.visualIndex;
                                if (fromIndex !== toIndex) {
                                    providerModel.move(fromIndex, toIndex, 1);
                                    page.syncConfig();
                                }
                            }
                        }

                        Kirigami.AbstractCard {
                            id: providerDelegate

                            readonly property int index: delegateRoot.index
                            readonly property string provider: delegateRoot.provider
                            readonly property string source: delegateRoot.source
                            readonly property bool enabled: delegateRoot.enabled
                            readonly property string account: delegateRoot.account
                            readonly property int accountIndex: delegateRoot.accountIndex
                            readonly property bool allAccounts: delegateRoot.allAccounts
                            readonly property bool showInCompactAll: delegateRoot.showInCompactAll
                            readonly property int compactBarLimit: delegateRoot.compactBarLimit
                            readonly property string compactBarIds: delegateRoot.compactBarIds
                            readonly property string compactColor: delegateRoot.compactColor
                            readonly property string apiKey: delegateRoot.apiKey
                            readonly property bool includeCost: delegateRoot.includeCost

                            property bool colorPickerOpen: false

                            property int visualIndex: delegateRoot.visualIndex

                            width: delegateRoot.width
                            z: dragMouseArea.drag.active ? 100 : 0
                            
                            anchors {
                                left: parent.left
                                right: parent.right
                                verticalCenter: parent.verticalCenter
                            }

                            opacity: dragMouseArea.drag.active ? 0.9 : 1.0
                            scale: dragMouseArea.drag.active ? 1.02 : 1.0
                            
                            Behavior on scale { NumberAnimation { duration: 100 } }
                            Behavior on opacity { NumberAnimation { duration: 100 } }

                            Drag.active: dragMouseArea.drag.active
                            Drag.source: providerDelegate
                            Drag.hotSpot.x: Kirigami.Units.gridUnit * 1.5
                            Drag.hotSpot.y: height / 2

                            states: [
                                State {
                                    when: dragMouseArea.drag.active
                                    ParentChange {
                                        target: providerDelegate
                                        parent: page
                                    }
                                    AnchorChanges {
                                        target: providerDelegate
                                        anchors {
                                            left: undefined
                                            right: undefined
                                            top: undefined
                                            bottom: undefined
                                            verticalCenter: undefined
                                            horizontalCenter: undefined
                                        }
                                    }
                                }
                            ]

                            padding: Kirigami.Units.smallSpacing

                            contentItem: RowLayout {
                                spacing: Kirigami.Units.smallSpacing

                                Item {
                                    id: dragHandle
                                    Layout.preferredWidth: Kirigami.Units.gridUnit * 1.5
                                    Layout.fillHeight: true
                                    Layout.alignment: Qt.AlignVCenter

                                    Kirigami.Icon {
                                        anchors.centerIn: parent
                                        source: "list-drag-handle-symbolic"
                                        width: Kirigami.Units.iconSizes.small
                                        height: Kirigami.Units.iconSizes.small
                                        color: dragMouseArea.pressed ? Kirigami.Theme.highlightColor : Kirigami.Theme.textColor
                                        opacity: dragMouseArea.containsMouse || dragMouseArea.pressed ? 1.0 : 0.4
                                        Behavior on opacity { NumberAnimation { duration: 150 } }
                                    }

                                    MouseArea {
                                        id: dragMouseArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        drag.target: providerDelegate
                                        drag.axis: Drag.YAxis
                                        cursorShape: drag.active ? Qt.ClosedHandCursor : (containsMouse ? Qt.OpenHandCursor : Qt.ArrowCursor)
                                    }
                                }

                                ColumnLayout {
                                    Layout.fillWidth: true
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

                                    // Antigravity Native Auth: browser Google OAuth
                                    RowLayout {
                                        Layout.fillWidth: true
                                        visible: providerDelegate.provider === "antigravity"
                                               && (providerDelegate.source === "native-auth"
                                                   || providerDelegate.source === "native"
                                                   || providerDelegate.source === "auto")

                                        QtControls.Button {
                                            text: i18n("Log in with browser")
                                            icon.name: "internet-web-browser"
                                            display: QtControls.AbstractButton.TextBesideIcon
                                            enabled: !page.antigravityAuthBusy
                                            onClicked: page.runAntigravityLogin()
                                        }

                                        QtControls.Button {
                                            text: i18n("Log out")
                                            icon.name: "system-log-out"
                                            display: QtControls.AbstractButton.TextBesideIcon
                                            enabled: !page.antigravityAuthBusy
                                            onClicked: page.runAntigravityLogout()
                                        }

                                        QtControls.Label {
                                            Layout.fillWidth: true
                                            text: page.antigravityAuthStatus
                                            color: Kirigami.Theme.disabledTextColor
                                            elide: Text.ElideRight
                                            wrapMode: Text.WordWrap
                                        }
                                    }

                                    RowLayout {
                                        Layout.fillWidth: true
                                        visible: providerDelegate.source === "api"
                                               || (providerDelegate.provider === "devin"
                                                   && (providerDelegate.source === "native"
                                                       || providerDelegate.source === "auto"))

                                        QtControls.Label {
                                            text: providerDelegate.provider === "devin" ? i18n("Bearer token:") : i18n("API Key:")
                                        }

                                        QtControls.TextField {
                                            id: apiKeyField
                                            Layout.fillWidth: true
                                            text: providerDelegate.apiKey
                                            echoMode: showKeyButton.checked ? TextInput.Normal : TextInput.Password
                                            placeholderText: providerDelegate.provider === "devin"
                                                ? i18n("Bearer token (or set DEVIN_BEARER_TOKEN)")
                                                : i18n("API key (optional if set in env)")
                                            onEditingFinished: page.setProviderProperty(providerDelegate.index, "apiKey", text)
                                        }

                                        QtControls.ToolButton {
                                            id: showKeyButton
                                            checkable: true
                                            icon.name: checked ? "password-show" : "password-hide"
                                            text: checked ? i18n("Hide key") : i18n("Show key")
                                            display: QtControls.AbstractButton.IconOnly
                                        }
                                    }

                                    // Devin: organization field + get-token helper
                                    RowLayout {
                                        Layout.fillWidth: true
                                        visible: providerDelegate.provider === "devin"

                                        QtControls.Label {
                                            text: i18n("Organization:")
                                        }

                                        QtControls.TextField {
                                            Layout.fillWidth: true
                                            text: providerDelegate.account
                                            placeholderText: i18n("org slug, org_... ID, or app.devin.ai/org/<slug> URL")
                                            onEditingFinished: page.setProviderProperty(providerDelegate.index, "account", text)
                                        }

                                        QtControls.Button {
                                            text: i18n("Get token")
                                            icon.name: "internet-web-browser"
                                            display: QtControls.AbstractButton.TextBesideIcon
                                            onClicked: {
                                                Qt.openUrlExternally("https://app.devin.ai/settings/usage");
                                                devinTokenHelp.visible = !devinTokenHelp.visible;
                                            }
                                        }
                                    }

                                    // Devin: collapsible console snippet for token extraction
                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        id: devinTokenHelp
                                        visible: false
                                        spacing: Kirigami.Units.smallSpacing

                                        Rectangle {
                                            Layout.fillWidth: true
                                            color: Qt.rgba(Kirigami.Theme.highlightColor.r, Kirigami.Theme.highlightColor.g, Kirigami.Theme.highlightColor.b, 0.12)
                                            radius: Kirigami.Units.cornerRadius
                                            implicitHeight: infoLabel.implicitHeight + Kirigami.Units.smallSpacing * 2

                                            QtControls.Label {
                                                id: infoLabel
                                                anchors.fill: parent
                                                anchors.margins: Kirigami.Units.smallSpacing
                                                text: i18n("Sign in to Devin, open DevTools (F12) → Console, paste the snippet below, and copy the token into the Bearer token field.")
                                                color: Kirigami.Theme.textColor
                                                wrapMode: Text.Wrap
                                            }
                                        }

                                        QtControls.Label {
                                            Layout.fillWidth: true
                                            text: "JSON.parse(Object.entries(localStorage).find(([k]) => k.includes('auth1_session'))?.[1] || '{}').token"
                                            font.family: "monospace"
                                            color: Kirigami.Theme.textColor
                                            background: Rectangle { color: Qt.rgba(Kirigami.Theme.textColor.r, Kirigami.Theme.textColor.g, Kirigami.Theme.textColor.b, 0.08); radius: Kirigami.Units.cornerRadius }
                                            wrapMode: Text.Wrap
                                            padding: Kirigami.Units.smallSpacing
                                        }

                                        QtControls.Button {
                                            text: i18n("Copy snippet")
                                            icon.name: "edit-copy"
                                            display: QtControls.AbstractButton.TextBesideIcon
                                            onClicked: {
                                                devinSnippetClipboard.text = "JSON.parse(Object.entries(localStorage).find(([k]) => k.includes('auth1_session'))?.[1] || '{}').token";
                                                devinSnippetClipboard.selectAll();
                                                devinSnippetClipboard.copy();
                                            }
                                        }
                                    }

                                    // Other providers: account filter toggle
                                    QtControls.Button {
                                        id: accountToggle
                                        Layout.fillWidth: true
                                        visible: providerDelegate.provider !== "devin"
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
                                        visible: providerDelegate.provider !== "devin" && accountToggle.checked

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

                                    RowLayout {
                                        Layout.fillWidth: true

                                        QtControls.CheckBox {
                                            text: i18n("Token & API costs")
                                            checked: providerDelegate.includeCost
                                            onToggled: page.setProviderProperty(providerDelegate.index, "includeCost", checked)
                                        }

                                        QtControls.CheckBox {
                                            text: i18n("Show in all-provider tray")
                                            checked: providerDelegate.showInCompactAll
                                            onToggled: page.setProviderProperty(providerDelegate.index, "showInCompactAll", checked)
                                        }

                                        Rectangle {
                                            id: compactColorSwatch
                                            Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                            Layout.preferredHeight: Kirigami.Units.iconSizes.small
                                            radius: 3
                                            color: page.normalizeColor(compactColorInput.text).length > 0
                                                   ? page.normalizeColor(compactColorInput.text)
                                                   : (providerDelegate.compactColor.length > 0 ? providerDelegate.compactColor : page.defaultProviderColor(providerDelegate.provider))
                                            border.width: 1
                                            border.color: page.normalizeColor(compactColorInput.text).length > 0
                                                          ? Kirigami.Theme.highlightColor
                                                          : Kirigami.Theme.disabledTextColor

                                            MouseArea {
                                                anchors.fill: parent
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: providerDelegate.colorPickerOpen = !providerDelegate.colorPickerOpen
                                            }
                                        }

                                        QtControls.TextField {
                                            id: compactColorInput
                                            Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                                            text: providerDelegate.compactColor
                                            placeholderText: page.defaultProviderColor(providerDelegate.provider)
                                            inputMask: "\\#HHHHHH;_"
                                            onEditingFinished: {
                                                const normalized = page.normalizeColor(text);
                                                // Keep the visible value in sync with stored config.
                                                text = normalized;
                                                page.setProviderProperty(providerDelegate.index, "compactColor", normalized);
                                            }
                                        }

                                        QtControls.ToolButton {
                                            icon.name: "edit-clear"
                                            text: i18n("Reset color")
                                            display: QtControls.AbstractButton.IconOnly
                                            enabled: providerDelegate.compactColor.length > 0
                                            onClicked: page.setProviderProperty(providerDelegate.index, "compactColor", "")
                                        }
                                    }

                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: Kirigami.Units.smallSpacing
                                        visible: page.barChoices(providerDelegate.provider).length === 0

                                        QtControls.Label {
                                            text: i18n("Maximum tray bars:")
                                            color: Kirigami.Theme.disabledTextColor
                                        }

                                        QtControls.SpinBox {
                                            from: 1
                                            to: 4
                                            value: providerDelegate.compactBarLimit
                                            onValueModified: page.setProviderProperty(providerDelegate.index, "compactBarLimit", value)
                                        }

                                        QtControls.Label {
                                            Layout.fillWidth: true
                                            text: i18n("Uses the limits and order reported by the provider")
                                            color: Kirigami.Theme.disabledTextColor
                                            wrapMode: Text.WordWrap
                                        }
                                    }

                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        spacing: 0
                                        visible: page.barChoices(providerDelegate.provider).length > 0

                                        QtControls.Label {
                                            text: i18n("Tray bars:")
                                            color: Kirigami.Theme.disabledTextColor
                                        }

                                        Repeater {
                                            model: page.barChoices(providerDelegate.provider)

                                            QtControls.CheckBox {
                                                required property var modelData
                                                text: modelData.title
                                                checked: page.barIdSelected(providerDelegate.compactBarIds, modelData.id)
                                                enabled: !checked || page.selectedBarIds(providerDelegate.compactBarIds).length > 1
                                                onToggled: page.setBarIdSelected(
                                                    providerDelegate.index,
                                                    providerDelegate.compactBarIds,
                                                    modelData.id,
                                                    checked)
                                            }
                                        }
                                    }

                                    ColumnLayout {
                                        Layout.fillWidth: true
                                        visible: providerDelegate.colorPickerOpen
                                        spacing: Kirigami.Units.smallSpacing

                                        RowLayout {
                                            Layout.fillWidth: true
                                            QtControls.Label {
                                                text: i18n("Color presets")
                                                color: Kirigami.Theme.disabledTextColor
                                            }

                                            Item { Layout.fillWidth: true }

                                            QtControls.ToolButton {
                                                text: i18n("Close")
                                                onClicked: providerDelegate.colorPickerOpen = false
                                            }
                                        }

                                        GridLayout {
                                            Layout.fillWidth: true
                                            columns: 8
                                            rowSpacing: Kirigami.Units.smallSpacing
                                            columnSpacing: Kirigami.Units.smallSpacing

                                            Rectangle {
                                                Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                                Layout.preferredHeight: Kirigami.Units.iconSizes.small
                                                radius: 3
                                                color: page.defaultProviderColor(providerDelegate.provider)
                                                border.width: 1
                                                border.color: providerDelegate.compactColor.length === 0
                                                                 ? Kirigami.Theme.highlightColor
                                                                 : Kirigami.Theme.disabledTextColor
                                                MouseArea {
                                                    anchors.fill: parent
                                                    cursorShape: Qt.PointingHandCursor
                                                    onClicked: {
                                                        page.setProviderProperty(providerDelegate.index, "compactColor", "")
                                                        providerDelegate.colorPickerOpen = false
                                                    }
                                                }
                                            }

                                            Repeater {
                                                model: page.providerColorPresets()
                                                delegate: Rectangle {
                                                    readonly property string preset: modelData

                                                    Layout.preferredWidth: Kirigami.Units.iconSizes.small
                                                    Layout.preferredHeight: Kirigami.Units.iconSizes.small
                                                    radius: 3
                                                    color: preset

                                                    border.width: 1
                                                    border.color: providerDelegate.compactColor === preset
                                                                   ? Kirigami.Theme.highlightColor
                                                                   : Kirigami.Theme.disabledTextColor

                                                    MouseArea {
                                                        anchors.fill: parent
                                                        cursorShape: Qt.PointingHandCursor
                                                        onClicked: {
                                                            page.setProviderProperty(providerDelegate.index, "compactColor", preset)
                                                            providerDelegate.colorPickerOpen = false
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        RowLayout {
                                            Layout.fillWidth: true
                                            spacing: Kirigami.Units.smallSpacing

                                            QtControls.Button {
                                                text: i18n("Custom color…")
                                                icon.name: "color-picker"
                                                onClicked: {
                                                    colorDialog.currentColor = page.defaultProviderColor(providerDelegate.provider);
                                                    colorDialog.open();
                                                }
                                            }

                                            QtControls.Label {
                                                Layout.fillWidth: true
                                                text: i18n("Use a custom accent if presets don't fit.")
                                                color: Kirigami.Theme.disabledTextColor
                                                wrapMode: Text.Wrap
                                            }
                                        }
                                    }
                                }
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
                                allAccounts: false,
                                showInCompactAll: true,
                                compactBarLimit: 4,
                                compactBarIds: page.defaultBarIds(provider.id),
                                compactColor: "",
                                apiKey: "",
                                includeCost: true
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
        // Keep an empty providerConfigs value in automatic-discovery mode.
        // The visible Codex row is the fallback and is persisted only after
        // the user edits provider settings.
    }

    function setProviderProperty(index, key, value) {
        providerModel.setProperty(index, key, value);
        syncConfig();
    }

    function syncConfig() {
        cfg_providerConfigs = serializeProviders();
        cfg_provider = enabledProviders().join(",");
        if (syncProviders.checked && !providerSyncApplying) {
            writeSharedProviders();
        }
    }

    function providerSyncScriptPath() {
        let scriptPath = Qt.resolvedUrl("../code/codexbar-provider-sync.mjs").toString();
        if (scriptPath.startsWith("file://")) {
            scriptPath = decodeURIComponent(scriptPath.slice("file://".length));
            if (scriptPath.startsWith("//") && !/^\/\/[A-Za-z]:\//.test(scriptPath)) {
                scriptPath = scriptPath.slice(1);
            }
        }
        return scriptPath;
    }

    function readSharedProviders() {
        providerSyncStatus = i18n("Loading shared providers…");
        providerSyncRunner.connectSource(shellQuote(providerSyncScriptPath()) + " --action read");
    }

    function writeSharedProviders() {
        providerSyncStatus = i18n("Saving shared providers…");
        providerSyncRunner.connectSource(
            shellQuote(providerSyncScriptPath())
                + " --action write --providers "
                + shellQuote(serializeProviders())
        );
    }

    function applySharedProviders(sharedProviders) {
        const localPresentation = {};
        for (let index = 0; index < providerModel.count; index += 1) {
            const local = providerModel.get(index);
            localPresentation[normalizedProviderId(local.provider)] = {
                showInCompactAll: local.showInCompactAll,
                compactBarLimit: local.compactBarLimit,
                compactBarIds: local.compactBarIds,
                compactColor: local.compactColor
            };
        }

        const parsed = parseProviderConfigs(JSON.stringify(sharedProviders || []));
        providerSyncApplying = true;
        providerModel.clear();
        for (let index = 0; index < parsed.length; index += 1) {
            const item = parsed[index];
            const presentation = localPresentation[normalizedProviderId(item.provider)];
            if (presentation) {
                item.showInCompactAll = presentation.showInCompactAll;
                item.compactBarLimit = presentation.compactBarLimit;
                item.compactBarIds = presentation.compactBarIds;
                item.compactColor = presentation.compactColor;
            }
            providerModel.append(item);
        }
        cfg_providerConfigs = serializeProviders();
        cfg_provider = enabledProviders().join(",");
        providerSyncApplying = false;
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
                allAccounts: item.allAccounts === true,
                showInCompactAll: item.showInCompactAll !== false,
                compactBarLimit: compactBarLimitFor(item),
                compactBarIds: compactBarIdsFor(item, provider.id),
                compactColor: normalizeColor(item.compactColor || ""),
                apiKey: String(item.apiKey || ""),
                includeCost: item.includeCost !== false
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
                allAccounts: item.allAccounts,
                showInCompactAll: item.showInCompactAll,
                compactBarLimit: item.compactBarLimit,
                compactBarIds: item.compactBarIds,
                compactColor: item.compactColor,
                apiKey: item.apiKey,
                includeCost: item.includeCost !== false
            });
        }
        return JSON.stringify(items);
    }

    function compactBarLimitFor(item) {
        const configured = Number(item.compactBarLimit);
        if (Number.isFinite(configured) && configured >= 1) {
            return Math.max(1, Math.min(4, Math.round(configured)));
        }
        // Migrate the old primary/secondary/tertiary switches without keeping
        // their provider-specific labels. At least one bar remains visible.
        let legacyCount = 0;
        legacyCount += item.compactBarPrimary !== false ? 1 : 0;
        legacyCount += item.compactBarSecondary !== false ? 1 : 0;
        legacyCount += item.compactBarTertiary !== false ? 1 : 0;
        // The old default enabled all three guessed slots. Treat that as
        // "show all" so providers with four or more real rows are not clipped.
        return legacyCount === 3 ? 4 : Math.max(1, legacyCount);
    }

    function barChoices(providerId) {
        try {
            const catalog = JSON.parse(String(plasmoid.configuration.compactBarCatalog || "{}"));
            const rows = catalog[normalizedProviderId(providerId)];
            if (Array.isArray(rows)) {
                return rows.filter(function(row) {
                    return row && String(row.id || "").length > 0;
                }).map(function(row) {
                    const id = String(row.id);
                    return { id: id, title: String(row.title || id) };
                });
            }
        } catch (error) {
            // A malformed cache should not prevent opening configuration.
        }
        return [];
    }

    function normalizedProviderId(providerId) {
        const normalized = String(providerId || "").toLowerCase().replace(/[-_]/g, "");
        const aliases = {
            abacusai: "abacus",
            alibabacodingplan: "alibaba",
            groqcloud: "groq"
        };
        return aliases[normalized] || normalized;
    }

    function defaultBarIds(providerId) {
        return JSON.stringify(barChoices(providerId).map(function(choice) { return choice.id; }));
    }

    function selectedBarIds(raw) {
        try {
            const parsed = JSON.parse(String(raw || "[]"));
            return Array.isArray(parsed) ? parsed.map(String) : [];
        } catch (error) {
            return [];
        }
    }

    function compactBarIdsFor(item, providerId) {
        const choices = barChoices(providerId);
        if (choices.length === 0) {
            return "[]";
        }
        const selected = selectedBarIds(item.compactBarIds);
        return selected.length > 0 ? JSON.stringify(selected) : defaultBarIds(providerId);
    }

    function barIdSelected(raw, id) {
        return selectedBarIds(raw).indexOf(id) !== -1;
    }

    function setBarIdSelected(index, raw, id, selected) {
        const ids = selectedBarIds(raw);
        const position = ids.indexOf(id);
        if (selected && position === -1) {
            ids.push(id);
        } else if (!selected && position !== -1 && ids.length > 1) {
            ids.splice(position, 1);
        }
        setProviderProperty(index, "compactBarIds", JSON.stringify(ids));
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

    function defaultProviderColor(providerId) {
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
        const normalized = String(providerId || "").toLowerCase().replace(/[-_]/g, "");
        const aliases = {
            alibabacodingplan: "alibaba",
            abacusai: "abacus",
            groqcloud: "groq"
        };
        return colors[aliases[normalized] || normalized] || "#4b929b";
    }

    function providerColorPresets() {
        // Mid-tone accents retain contrast without glowing against dark themes
        // or becoming too heavy against light themes.
        return [
            // Blues / cyans
            "#477fc2", "#5879b8", "#4d88ad", "#4b929b",
            // Greens / teals
            "#398979", "#3d8d76", "#4a9173", "#55976b",
            // Purples
            "#8972b5", "#666db0", "#8c68b7", "#756ca8",
            // Oranges / ambers
            "#bd7434", "#b87243", "#b57b32", "#a77c48",
            // Reds / pinks
            "#b96170", "#bd684e", "#ae628a", "#a76572",
            // Warm / cool low-chroma accents
            "#b57861", "#8b766e", "#667f91", "#72877a"
        ];
    }

    function normalizeColor(value) {
        const text = String(value || "").trim();
        return /^#[0-9a-fA-F]{6}$/.test(text) ? text : "";
    }

    function resolveNativeCliPath() {
        let path = Qt.resolvedUrl("../code/codexbar-plasmoid").toString();
        if (path.startsWith("file://")) {
            path = decodeURIComponent(path.slice("file://".length));
            // file:///home/... → /home/...
            if (/^\/[A-Za-z]:\//.test(path)) {
                // Windows-style file URL; keep as-is.
            } else if (path.startsWith("//")) {
                path = path.slice(1);
            }
        }
        return path;
    }

    function shellQuote(value) {
        return "'" + String(value).replace(/'/g, "'\\''") + "'";
    }

    function runAntigravityLogin() {
        if (page.antigravityAuthBusy) {
            return;
        }
        const cli = resolveNativeCliPath();
        page.antigravityAuthBusy = true;
        page.antigravityAuthStatus = i18n("Waiting for browser login…");
        antigravityAuthRunner.connectSource(
            shellQuote(cli) + " login --provider antigravity --timeout 300"
        );
    }

    function runAntigravityLogout() {
        if (page.antigravityAuthBusy) {
            return;
        }
        const cli = resolveNativeCliPath();
        page.antigravityAuthBusy = true;
        page.antigravityAuthStatus = i18n("Logging out…");
        antigravityAuthRunner.connectSource(
            shellQuote(cli) + " logout --provider antigravity --all"
        );
    }

    // Hidden TextEdit for clipboard operations (Devin token snippet copy)
    TextEdit {
        id: devinSnippetClipboard
        visible: false
        width: 0
        height: 0
    }

    Plasma5Support.DataSource {
        id: providerSyncRunner
        engine: "executable"
        connectedSources: []
        onNewData: function(sourceName, data) {
            disconnectSource(sourceName);
            const output = String(data.stdout || data["stdout"] || "").trim();
            if (!output.length) {
                page.providerSyncStatus = i18n("Provider sync returned no data");
                return;
            }
            try {
                const result = JSON.parse(output);
                if (result.ok === false) {
                    page.providerSyncStatus = result.error || i18n("Provider sync failed");
                    return;
                }
                if (sourceName.indexOf("--action read") !== -1) {
                    if (result.exists && Array.isArray(result.providers) && result.providers.length > 0) {
                        page.applySharedProviders(result.providers);
                        page.providerSyncStatus = i18n("Using shared providers");
                    } else {
                        page.writeSharedProviders();
                    }
                } else {
                    page.providerSyncStatus = i18n("Shared providers saved");
                }
            } catch (error) {
                page.providerSyncStatus = String(error);
            }
        }
    }

    Plasma5Support.DataSource {
        id: antigravityAuthRunner
        engine: "executable"
        connectedSources: []
        onNewData: function(sourceName, data) {
            const exitCode = Number(data["exit code"] ?? data.exitCode ?? 1);
            const stdout = String(data.stdout || "").trim();
            const stderr = String(data.stderr || "").trim();
            if (exitCode === 0) {
                page.antigravityAuthStatus = stdout.length > 0
                    ? stdout.split("\n")[0]
                    : i18n("Done.");
            } else {
                const detail = stderr || stdout || i18n("Login command failed");
                page.antigravityAuthStatus = detail.split("\n")[0];
            }
            page.antigravityAuthBusy = false;
            disconnectSource(sourceName);
        }
    }

    QtDialogs.ColorDialog {
        id: colorDialog
        title: i18n("Pick provider color")
        modality: Qt.ApplicationModal
        onAccepted: {
            // Convert selected color to #RRGGBB and apply to the currently selected provider row.
            const hex = "#" + Qt.rgba(color.r, color.g, color.b, 1.0).toString().slice(1, 7);
            const normalized = normalizeColor(hex);
            if (normalized.length > 0 && providersListView.currentIndex >= 0 && providersListView.currentIndex < providerModel.count) {
                providerModel.setProperty(providersListView.currentIndex, "compactColor", normalized);
                syncConfig();
            }
        }
    }
}

