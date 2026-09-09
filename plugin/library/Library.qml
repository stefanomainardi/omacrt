import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import qs.Ui

// The library overlay: a full screen surface over the desktop where the game
// collection is managed. Sources (the folders the scan reads, disks that
// look like collections), the systems with their games and cores (with an
// install offer for a missing core), the folders of the systems that read
// one, the BIOS files and where to import them from, and the folders the
// scan could not place. Every button runs one `omacrt` command; the
// scan runs here and shows the folder it reads, the few that need a
// password open a floating terminal.
//
// An Omarchy shell plugin of kind `overlay`, its own id next to the bar
// widget: the shell mounts it on `summon` and hands the payload to `open()`.
// The bar panel summons it with the path of its `omacrt` binary.
Item {
  id: root

  property var shell: null
  property var manifest: null
  // Resolved once, from this plugin's own directory: the overlay is installed
  // as `<id>.library` beside `<id>`, which holds `bin/omacrt`.
  readonly property string ownHelper:
    Qt.resolvedUrl("../io.github.stefanomainardi.omacrt/bin/omacrt").toString().replace("file://", "")
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
  // The scan runs here rather than in a terminal: this is the folder it is
  // reading now, and the summary it printed when it ended.
  property string scanDir: ""
  property string scanResult: ""

  readonly property var systems: (lib.systems || []).filter(function(s) { return !s.video })
  // Systems that read one folder of their own instead of the index.
  readonly property var folderSystems: (lib.systems || []).filter(function(s) { return s.dir && s.dir !== "index" })
  readonly property var roots: lib.roots || []
  readonly property var newDisks: discovered.filter(function(d) { return !d.root })
  readonly property var missingCores: cores.filter(function(c) { return !c.installed })
  readonly property var biosMissing: (biosReport.items || []).filter(function(i) { return i.required && i.relevant && !i.present })
  readonly property int games: (lib.index && lib.index.games) || 0
  readonly property string scannedAt: (lib.index && lib.index.scanned_at) ? String(lib.index.scanned_at).slice(0, 16).replace("T", " ") : ""

  // The payload carries data, never the program to run. `helper` used to be
  // read from it, and `refresh()` starts six processes with it the moment the
  // overlay opens, so any program running as the user could have summoned this
  // with a helper of its own and had it executed six times inside the shell.
  // The binary is the one that sits beside the bar plugin, resolved from this
  // file's own location, which is where the installer puts it.
  function open(payloadJson) {
    var payload = {}
    try { payload = JSON.parse(String(payloadJson || "{}")) || {} } catch (e) { payload = {} }
    root.helper = root.ownHelper
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
      root.shell.hide((root.manifest && root.manifest.id) || "io.github.stefanomainardi.omacrt.library")
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

  // One omacrt command, quiet; the overlay refreshes when it ends.
  function act(args) {
    if (!root.helper || actionProc.running) return
    root.errorMessage = ""
    root.busy = args.join(" ")
    actionProc.command = [root.helper].concat(args)
    actionProc.running = true
  }

  // A command with progress or a password: a floating terminal.
  // The receiver runs this through a shell - the `; echo; read` tail only
  // works there - so everything variable in `cmd` has to arrive quoted. `q`
  // is what does it, and every caller below uses it.
  function inTerminal(cmd) {
    Quickshell.execDetached(["omarchy-launch-floating-terminal-with-presentation",
      cmd + "; echo; read -n 1 -s -r -p 'Press any key to close'"])
  }

  function q(s) { return "'" + String(s).replace(/'/g, "'\\''") + "'" }

  // What a source folder is called on screen. The full path is what the
  // commands take, but it carries the user's name and the label of whatever
  // disk the collection sits on, and this overlay ends up in screenshots.
  // Home becomes `~`, a removable disk becomes its own name.
  function short(p) {
    var path = String(p)
    var home = Quickshell.env("HOME") || ""
    if (home.length > 0 && path.indexOf(home + "/") === 0) return "~" + path.slice(home.length)
    var m = path.match(/^\/(?:run\/media|media)\/[^/]+\/(.+)$/)
    if (m) return m[1]
    return path
  }

  // The scan in the background, with the folder it reads shown as it goes.
  function scan(dir) {
    if (scanProc.running) return
    root.errorMessage = ""
    root.scanResult = ""
    root.scanDir = dir || "every source"
    scanProc.command = dir
      ? [root.helper, "library", "scan", dir, "--progress"]
      : [root.helper, "library", "scan", "--progress"]
    scanProc.running = true
  }

  function stopScan() {
    if (scanProc.running) scanProc.signal(15)
  }

  function installCore(c) {
    var cmd = "omarchy pkg " + (c.aur ? "aur " : "") + "add " + root.q(c.package)
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
    id: scanProc
    stdout: SplitParser {
      onRead: function(line) {
        var l = String(line || "").trim()
        if (l.indexOf("scanning ") === 0) root.scanDir = l.substring(9)
        else if (l) root.scanResult = l
      }
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var e = String(text || "").trim()
        if (e) root.errorMessage = e
      }
    }
    onRunningChanged: {
      if (!running) {
        root.scanDir = ""
        root.refresh()
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
    WlrLayershell.namespace: "omacrt-library"
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
      // As tall as what is in it, up to the screen. A fixed height cut the
      // last row in half on one collection and left a gap on another, and
      // the number of systems is not something this can know in advance.
      height: Math.min(
        2 * Style.spacing.panelPadding + 3 * Style.space(10)
          + head.height + hairline.height + content.implicitHeight + footer.height,
        parent.height - Style.space(80))
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
            id: head
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
          Line { id: hairline }

          Flickable {
            width: parent.width
            // What the list needs, or what is left of the screen, whichever
            // is smaller: the scrollbar appears only when it is earned.
            height: Math.min(content.implicitHeight,
                             parent.height - y - footer.height - Style.space(10))
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
                      text: root.short(modelData.path)
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
                      text: root.short(modelData.path)
                      color: root.accent
                      width: parent.width - Style.space(220)
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Small { text: "looks like a collection"; width: Style.space(90); anchors.verticalCenter: parent.verticalCenter }
                    Act {
                      text: "Adopt and scan"
                      enabled: !scanProc.running
                      onClicked: root.scan(modelData.path)
                    }
                  }
                }
                Row {
                  width: parent.width
                  spacing: Style.space(8)
                  TextField {
                    id: rootField
                    // Room for both buttons beside it: the old reservation
                    // of 230 cut the second one off the panel.
                    width: parent.width - Style.space(390)
                    text: root.newRoot
                    foreground: root.fg
                    font.family: root.mono
                    onTextChanged: root.newRoot = text
                    onAccepted: if (root.newRoot.trim()) root.scan(root.newRoot.trim())
                  }
                  Act {
                    id: addBtn
                    text: "Add folder and scan"
                    enabled: root.newRoot.trim() !== "" && !actionProc.running && !scanProc.running
                    onClicked: root.scan(root.newRoot.trim())
                  }
                  Act {
                    id: rescanBtn
                    text: "Rescan sources"
                    enabled: root.roots.length > 0 && !actionProc.running && !scanProc.running
                    onClicked: root.scan("")
                  }
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

              // ---------------------------------------------- folder systems
              Section {
                visible: root.folderSystems.length > 0
                title: "FOLDERS"
                hint: "systems that read one folder of their own, films among them"
                Repeater {
                  model: root.folderSystems
                  Row {
                    id: dirRow
                    required property var modelData
                    // Shown the way the sources are, `~/Videos` rather than
                    // a path with somebody's name in it. The CLI expands a
                    // tilde on the way in, and `default_systems` writes them
                    // this way itself.
                    property string edited: root.short(modelData.dir)
                    width: parent.width
                    spacing: Style.space(8)
                    Mono {
                      text: modelData.label + "  " + modelData.name
                      width: Style.space(250)
                      color: modelData.exists ? root.fg : root.urgent
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Mono {
                      text: String(modelData.games)
                      width: Style.space(70)
                      horizontalAlignment: Text.AlignRight
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    TextField {
                      width: parent.width - Style.space(490)
                      text: dirRow.edited
                      foreground: modelData.exists ? root.fg : root.urgent
                      font.family: root.mono
                      onTextChanged: dirRow.edited = text
                      onAccepted: if (dirRow.edited.trim()) root.act(["library", "set", modelData.name, "dir=" + dirRow.edited.trim()])
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Act {
                      text: "Use this folder"
                      enabled: dirRow.edited.trim() !== "" && dirRow.edited.trim() !== root.short(modelData.dir) && !actionProc.running
                      onClicked: root.act(["library", "set", modelData.name, "dir=" + dirRow.edited.trim()])
                    }
                  }
                }
              }

              // -------------------------------------------------------- BIOS
              Section {
                title: "BIOS"
                hint: root.biosMissing.length > 0
                  ? (root.biosMissing.length + " required file(s) missing in " + root.short(root.biosReport.system_dir || ""))
                  : ("all required files present in " + root.short(root.biosReport.system_dir || ""))
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
                      text: root.short(modelData.path)
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
                  Act { text: "Full BIOS list"; onClicked: root.inTerminal(root.q(root.helper) + " bios") }
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
              Act { text: "Doctor"; onClicked: root.inTerminal(root.q(root.helper) + " doctor") }
              Act { text: "Reload"; onClicked: root.refresh() }
              Act { visible: scanProc.running; text: "Stop the scan"; onClicked: root.stopScan() }
              Text {
                width: parent.width - Style.space(200)
                anchors.verticalCenter: parent.verticalCenter
                text: scanProc.running ? ("scanning  " + root.scanDir)
                    : (root.busy !== "" ? ("omacrt " + root.busy + " …")
                    : (root.scanResult !== "" ? root.scanResult
                    : (root.errorMessage !== "" ? root.errorMessage : root.note)))
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
