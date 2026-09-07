import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import qs.Ui

// The library overlay: a full screen surface over the desktop where the game
// collection is managed. Sources (the folders the scan reads, disks that
// look like collections), the systems with their games and cores (with an
// install offer for a missing core), the BIOS files and where to import them
// from, and the folders the scan could not place. Every button runs one
// `omarchy-crt` command; long ones open a floating terminal so their
// progress shows.
//
// An Omarchy shell plugin of kind `overlay`, its own id next to the bar
// widget: the shell mounts it on `summon` and hands the payload to `open()`.
// The bar panel summons it with the path of its `omarchy-crt` binary.
Item {
  id: root

  property var shell: null
  property var manifest: null
  property string helper: ""
  property bool opened: false

  readonly property color fg: Color.menu.text
  readonly property color bg: Color.menu.background
  readonly property color accent: Color.accent
  readonly property color urgent: Color.urgent
  readonly property color muted: Qt.darker(root.fg, 1.6)
  readonly property color line: Color.menu.border
  readonly property string mono: Style.font.menuFamily

  property var lib: ({})
  property var cores: []
  property var available: []
  property var discovered: []
  property var biosReport: ({})
  property var biosFolders: []
  property var unknownDirs: []
  property string busy: ""
  property string note: ""
  property string errorMessage: ""
  property string newRoot: ""
  property var catalog: []

  readonly property var systems: (lib.systems || []).filter(function(s) { return !s.video })
  readonly property var roots: lib.roots || []
  readonly property var newDisks: discovered.filter(function(d) { return !d.root })
  readonly property var missingCores: cores.filter(function(c) { return !c.installed })
  readonly property var biosMissing: (biosReport.items || []).filter(function(i) { return i.required && i.relevant && !i.present })
  readonly property int games: (lib.index && lib.index.games) || 0
  readonly property string scannedAt: (lib.index && lib.index.scanned_at) ? String(lib.index.scanned_at).slice(0, 16).replace("T", " ") : ""

  function open(payloadJson) {
    var payload = {}
    try { payload = JSON.parse(String(payloadJson || "{}")) || {} } catch (e) { payload = {} }
    if (payload.helper) root.helper = String(payload.helper)
    if (!root.helper) root.helper = Quickshell.env("HOME") + "/.local/bin/omarchy-crt"
    root.opened = true
    root.errorMessage = ""
    root.note = ""
    refresh()
    Qt.callLater(function() { keys.forceActiveFocus() })
  }

  function close() { root.opened = false }

  // Esc, the scrim or the button: closed here and told to the shell so its
  // open state follows.
  function dismiss() {
    root.opened = false
    if (root.shell && typeof root.shell.hide === "function")
      root.shell.hide((root.manifest && root.manifest.id) || "io.github.stefanomainardi.omarchy-crt.library")
  }

  function refresh() {
    if (!root.helper) return
    libProc.running = true
    coresProc.running = true
    discoverProc.running = true
    biosProc.running = true
    biosDiscProc.running = true
    unknownProc.running = true
    if (root.catalog.length === 0) catalogProc.running = true
  }

  // One omarchy-crt command, quiet; the overlay refreshes when it ends.
  function act(args) {
    if (!root.helper || actionProc.running) return
    root.errorMessage = ""
    root.busy = args.join(" ")
    actionProc.command = [root.helper].concat(args)
    actionProc.running = true
  }

  // A command with progress or a password: a floating terminal.
  function inTerminal(cmd) {
    Quickshell.execDetached(["omarchy-launch-floating-terminal-with-presentation",
      cmd + "; echo; read -n 1 -s -r -p 'Press any key to close'"])
  }

  function q(s) { return "'" + String(s).replace(/'/g, "'\\''") + "'" }

  function scan(dir) {
    root.inTerminal(root.helper + " library scan" + (dir ? " " + q(dir) : ""))
  }

  function installCore(c) {
    var cmd = c.aur ? ("omarchy pkg aur add " + c.package) : ("omarchy pkg add " + c.package)
    root.inTerminal(cmd)
  }

  function parse(text, fallback) {
    try { return JSON.parse(String(text || "")) } catch (e) { return fallback }
  }

  Process {
    id: libProc
    command: [root.helper, "library", "--json"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.lib = root.parse(text, {}) }
  }
  Process {
    id: coresProc
    command: [root.helper, "library", "cores", "--json"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var d = root.parse(text, {})
        root.cores = Array.isArray(d) ? d : (d.systems || [])
        root.available = (d && d.available) || []
      }
    }
  }
  Process {
    id: discoverProc
    command: [root.helper, "library", "discover", "--json"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.discovered = root.parse(text, []) }
  }
  Process {
    id: biosProc
    command: [root.helper, "bios", "--json"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.biosReport = root.parse(text, {}) }
  }
  Process {
    id: biosDiscProc
    command: [root.helper, "bios", "discover", "--json"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.biosFolders = root.parse(text, []) }
  }
  Process {
    id: catalogProc
    command: [root.helper, "library", "systems"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var out = []
        String(text || "").split("\n").forEach(function(l) {
          var m = l.match(/^(\S+)\s+(.+?)\s{2,}\S+\s+\S+$/)
          if (m) out.push({ value: m[1], label: m[2].trim() + "  (" + m[1] + ")" })
        })
        root.catalog = out
      }
    }
  }
  Process {
    id: unknownProc
    command: [root.helper, "library", "unknown"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var dirs = {}
        String(text || "").split("\n").forEach(function(l) {
          if (!l.trim()) return
          var d = l.substring(0, l.lastIndexOf("/"))
          dirs[d] = (dirs[d] || 0) + 1
        })
        var out = []
        for (var d in dirs) out.push({ dir: d, files: dirs[d], system: "" })
        out.sort(function(a, b) { return b.files - a.files })
        root.unknownDirs = out.slice(0, 40)
      }
    }
  }
  Process {
    id: actionProc
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.note = String(text || "").trim().split("\n").slice(-1)[0] || ""
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.errorMessage = String(text || "").trim()
    }
    onRunningChanged: if (!running) { root.busy = ""; root.refresh() }
  }

  component Mono: Text {
    color: root.fg
    font.family: root.mono
    font.pixelSize: Style.font.body
    elide: Text.ElideMiddle
  }
  component Small: Text {
    color: root.muted
    font.family: root.mono
    font.pixelSize: Style.font.caption
    elide: Text.ElideMiddle
  }
  component Act: Button {
    bordered: true
    focusable: true
    foreground: root.fg
    fontFamily: root.mono
    enabled: !actionProc.running
  }
  component Section: Column {
    property string title: ""
    property string hint: ""
    width: parent.width
    spacing: Style.space(6)
    Row {
      width: parent.width
      spacing: Style.space(10)
      PanelSectionHeader { text: parent.parent.title; foreground: root.fg }
      Small { text: parent.parent.hint; anchors.verticalCenter: parent.verticalCenter }
    }
  }
  component Line: Rectangle { width: parent.width; height: 1; color: root.line; opacity: 0.5 }

  PanelWindow {
    id: win
    visible: root.opened
    anchors { top: true; bottom: true; left: true; right: true }
    color: "transparent"
    WlrLayershell.namespace: "omarchy-crt-library"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: root.opened ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None
    exclusionMode: ExclusionMode.Ignore

    Rectangle {
      anchors.fill: parent
      color: Color.menu.scrim
      MouseArea { anchors.fill: parent; onClicked: root.dismiss() }
    }

    Rectangle {
      id: card
      width: Math.min(Style.space(920), parent.width - Style.space(80))
      height: Math.min(Style.space(680), parent.height - Style.space(80))
      anchors.centerIn: parent
      radius: Style.cornerRadius
      color: root.bg
      border.width: Style.space(2)
      border.color: root.line
      clip: true
      MouseArea { anchors.fill: parent; onClicked: {} }

      Item {
        id: keys
        anchors.fill: parent
        focus: true
        Keys.onEscapePressed: root.dismiss()

        Column {
          anchors.fill: parent
          anchors.margins: Style.spacing.panelPadding
          spacing: Style.space(10)

          // --------------------------------------------------------- header
          Row {
            width: parent.width
            spacing: Style.space(12)
            Column {
              width: parent.width - closeBtn.width - Style.space(12)
              spacing: Style.space(2)
              Text {
                text: "Library"
                color: root.fg
                font.family: root.mono
                font.pixelSize: Style.font.title
                font.bold: true
              }
              Small {
                width: parent.width
                text: root.games > 0
                  ? (root.games + " games indexed" + (root.scannedAt ? ", scanned " + root.scannedAt : "") + "  ·  " + root.systems.length + " systems")
                  : "no index yet: add a folder or adopt a disk below, then scan"
              }
            }
            Act { id: closeBtn; text: "Close"; onClicked: root.dismiss(); anchors.verticalCenter: parent.verticalCenter }
          }
          Line {}

          Flickable {
            width: parent.width
            height: parent.height - y - footer.height - Style.space(10)
            contentWidth: width
            contentHeight: content.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            Column {
              id: content
              width: parent.width
              spacing: Style.space(16)

              // ----------------------------------------------------- sources
              Section {
                title: "SOURCES"
                hint: "folders the scan reads, whatever their layout"
                Repeater {
                  model: root.roots
                  Row {
                    required property var modelData
                    width: parent.width
                    spacing: Style.space(8)
                    Mono {
                      text: modelData.path
                      color: modelData.mounted ? root.fg : root.muted
                      width: parent.width - Style.space(220)
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Small { text: modelData.mounted ? "mounted" : "not mounted"; width: Style.space(90); anchors.verticalCenter: parent.verticalCenter }
                    Act { text: "Remove"; onClicked: root.act(["library", "roots", "remove", modelData.path]) }
                  }
                }
                Repeater {
                  model: root.newDisks
                  Row {
                    required property var modelData
                    width: parent.width
                    spacing: Style.space(8)
                    Mono {
                      text: modelData.path
                      color: root.accent
                      width: parent.width - Style.space(220)
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Small { text: "looks like a collection"; width: Style.space(90); anchors.verticalCenter: parent.verticalCenter }
                    Act { text: "Adopt and scan"; onClicked: root.scan(modelData.path) }
                  }
                }
                Row {
                  width: parent.width
                  spacing: Style.space(8)
                  TextField {
                    id: rootField
                    width: parent.width - Style.space(230)
                    text: root.newRoot
                    foreground: root.fg
                    font.family: root.mono
                    onTextChanged: root.newRoot = text
                    onAccepted: if (root.newRoot.trim()) root.scan(root.newRoot.trim())
                  }
                  Act {
                    text: "Add folder and scan"
                    enabled: root.newRoot.trim() !== "" && !actionProc.running
                    onClicked: root.scan(root.newRoot.trim())
                  }
                  Act { text: "Rescan"; enabled: root.roots.length > 0 && !actionProc.running; onClicked: root.scan("") }
                }
              }

              // ----------------------------------------------------- systems
              Section {
                title: "SYSTEMS"
                hint: root.missingCores.length > 0
                  ? (root.missingCores.length + " system(s) wait for a core")
                  : "every system has its core"
                Row {
                  width: parent.width
                  spacing: Style.space(8)
                  Small { text: "system"; width: Style.space(250) }
                  Small { text: "games"; width: Style.space(70); horizontalAlignment: Text.AlignRight }
                  Small { text: "core"; width: Style.space(180) }
                  Small { text: "" }
                }
                Repeater {
                  model: root.systems
                  Row {
                    id: sysRow
                    required property var modelData
                    readonly property var coreInfo: root.cores.find(function(c) { return c.system === modelData.name }) || null
                    width: parent.width
                    spacing: Style.space(8)
                    Mono {
                      text: modelData.label + "  " + modelData.name
                      width: Style.space(250)
                      color: modelData.games > 0 ? root.fg : root.muted
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Mono {
                      text: String(modelData.games)
                      width: Style.space(70)
                      horizontalAlignment: Text.AlignRight
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    // The core: a picker over what is installed, or the missing
                    // one's name in red with the install offer next to it.
                    Dropdown {
                      visible: modelData.core && root.available.length > 0
                      width: Style.space(180)
                      showLabel: false
                      value: modelData.core_name || ""
                      options: root.available
                      foreground: root.fg
                      background: root.bg
                      fontFamily: root.mono
                      onChanged: function(v) {
                        if (v && v !== modelData.core_name) root.act(["library", "set", modelData.name, "core=" + v])
                      }
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Mono {
                      visible: !modelData.core || root.available.length === 0
                      text: modelData.core_name || ""
                      width: Style.space(180)
                      color: modelData.core ? root.fg : root.urgent
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Act {
                      visible: !modelData.core && !!sysRow.coreInfo
                      text: "Install " + (sysRow.coreInfo ? sysRow.coreInfo.package : "") + (sysRow.coreInfo && sysRow.coreInfo.aur ? "  (AUR)" : "")
                      onClicked: root.installCore(sysRow.coreInfo)
                    }
                    Small {
                      visible: modelData.core && modelData.unknown > 0
                      text: modelData.unknown + " file(s) with an unknown extension"
                      anchors.verticalCenter: parent.verticalCenter
                    }
                  }
                }
              }

              // -------------------------------------------------------- BIOS
              Section {
                title: "BIOS"
                hint: root.biosMissing.length > 0
                  ? (root.biosMissing.length + " required file(s) missing in " + (root.biosReport.system_dir || ""))
                  : ("all required files present in " + (root.biosReport.system_dir || ""))
                Repeater {
                  model: root.biosMissing
                  Row {
                    required property var modelData
                    width: parent.width
                    spacing: Style.space(8)
                    Mono { text: modelData.file; width: Style.space(200); color: root.urgent }
                    Mono { text: modelData.description; width: Style.space(320); color: root.muted }
                    Small { text: modelData.system }
                  }
                }
                Repeater {
                  model: root.biosFolders
                  Row {
                    required property var modelData
                    width: parent.width
                    spacing: Style.space(8)
                    Mono {
                      text: modelData.path
                      width: parent.width - Style.space(330)
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Small { text: modelData.files + " known file(s)"; width: Style.space(110); anchors.verticalCenter: parent.verticalCenter }
                    Act {
                      text: "Import from here"
                      tooltipText: "copies the files the cores expect, and the known BIOS folders, into RetroArch's system directory"
                      onClicked: root.act(["bios", "import", modelData.path, "--all"])
                    }
                  }
                }
                Row {
                  width: parent.width
                  spacing: Style.space(8)
                  Act { text: "Full BIOS list"; onClicked: root.inTerminal(root.helper + " bios") }
                }
              }

              // ------------------------------------------------ unknown folders
              Section {
                visible: root.unknownDirs.length > 0
                title: "UNPLACED FOLDERS"
                hint: "the scan did not recognise these; name the system, then rescan"
                Repeater {
                  model: root.unknownDirs
                  Row {
                    id: unkRow
                    required property var modelData
                    required property int index
                    property string chosen: ""
                    width: parent.width
                    spacing: Style.space(8)
                    Mono {
                      text: modelData.dir
                      width: parent.width - Style.space(420)
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Small { text: modelData.files + " file(s)"; width: Style.space(70); anchors.verticalCenter: parent.verticalCenter }
                    Dropdown {
                      width: Style.space(230)
                      showLabel: false
                      value: unkRow.chosen
                      options: root.catalog
                      foreground: root.fg
                      background: root.bg
                      fontFamily: root.mono
                      onChanged: function(v) { unkRow.chosen = v }
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Act {
                      text: "Assign"
                      enabled: unkRow.chosen !== "" && !actionProc.running
                      onClicked: root.act(["library", "assign", modelData.dir, unkRow.chosen])
                    }
                  }
                }
              }
            }
          }

          // --------------------------------------------------------- footer
          Column {
            id: footer
            width: parent.width
            spacing: Style.space(6)
            Line {}
            Row {
              width: parent.width
              spacing: Style.space(8)
              Act { text: "Doctor"; onClicked: root.inTerminal(root.helper + " doctor") }
              Act { text: "Refresh"; onClicked: root.refresh() }
              Text {
                width: parent.width - Style.space(200)
                anchors.verticalCenter: parent.verticalCenter
                text: root.busy !== "" ? ("omarchy-crt " + root.busy + " …")
                    : (root.errorMessage !== "" ? root.errorMessage : root.note)
                color: root.errorMessage !== "" ? root.urgent : root.muted
                font.family: root.mono
                font.pixelSize: Style.font.caption
                elide: Text.ElideRight
              }
            }
          }
        }
      }
    }
  }
}
