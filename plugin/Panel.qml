import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// The OmaCRT panel: a little tube inside the bar. The hero is drawn as a
// television on-screen display, scanlines and all, and reports what the
// `omacrt` binary knows. Buttons below are the remote control.
Panel {
  id: root
  moduleName: "io.github.stefanomainardi.omacrt"
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null
  property string helper: ""
  property var status: ({})
  property string busy: ""
  property string errorMessage: ""
  property string lastNote: ""
  property bool watching: false
  property bool cursorOn: true
  property string watchUrl: ""

  readonly property color fg: root.bar ? root.bar.foreground : Color.foreground
  readonly property color bg: root.bar ? root.bar.background : Color.background
  readonly property color accent: Color.accent
  readonly property color urgent: root.bar ? root.bar.urgent : Color.urgent
  readonly property color muted: Qt.darker(root.fg, 1.6)
  readonly property color phosphor: "#7ee787"
  readonly property string mono: root.bar ? root.bar.fontFamily : Style.font.family

  readonly property var conn: status.connector || null
  readonly property var mode: status.mode || null
  readonly property var dac: status.dac || ({})
  readonly property var audio: status.audio || null
  readonly property var shell: status.shell || ({})
  readonly property var bios: status.bios || ({})
  readonly property var library: status.library || ({})
  readonly property bool connected: !!(conn && conn.connected)
  readonly property bool active: status.active === true
  readonly property bool locked: !!(dac.present && dac.lock === "locked")
  readonly property bool lockLost: root.active && !!(dac.present && dac.lock === "lost")
  readonly property string standard: String(status.standard || "ntsc")
  readonly property string csync: String(dac.csync || "")
  readonly property bool audioOnTv: !!(audio && audio.routed)
  readonly property bool audioAll: !!(audio && audio["default"])
  readonly property bool shellRunning: shell.running === true
  readonly property int biosMissing: Number(bios.missing || 0)
  readonly property var missingCores: (library.missing_cores || [])
  readonly property int volume: Number((audio && audio.volume) || 100)
  property int volumeShown: -1

  function openLibrary() {
    root.close()
    // No `helper` in the payload: the overlay resolves the binary from its own
    // directory, so a summon from anywhere cannot name the program it runs.
    Quickshell.execDetached(["omarchy-shell", "shell", "summon", root.moduleName + ".library", "{}"])
  }

  function open() {
    root.controller.show()
    refresh()
  }
  function close() { root.controller.hide() }
  function toggle() { root.opened ? close() : open() }

  function switchPanel(direction) {
    if (root.bar && typeof root.bar.switchPanelFrom === "function")
      return root.bar.switchPanelFrom(root.hostWidget || root, direction)
    return false
  }

  function refresh() {
    if (!root.helper || statusProc.running) return
    statusProc.command = [root.helper, "status", "--json"]
    statusProc.running = true
    if (!watchProc.running) watchProc.running = true
  }

  function runAction(args) {
    if (!root.helper || actionProc.running) return
    root.errorMessage = ""
    root.busy = args.join(" ")
    actionProc.command = [root.helper].concat(args)
    actionProc.running = true
  }

  // The receiver runs this through a shell - the `; echo; read` tail only
  // works there - so anything variable in `cmd` arrives quoted through `q`.
  function inTerminal(cmd) {
    Quickshell.execDetached(["omarchy-launch-floating-terminal-with-presentation",
      cmd + "; echo; read -n 1 -s -r -p 'Press any key to close'"])
  }

  function q(s) { return "'" + String(s).replace(/'/g, "'\\''") + "'" }

  function toggleWatch() {
    if (root.watching) {
      // The full path, not the bare words: `pkill -f "omacrt dac watch"`
      // matches any command line carrying that phrase, an editor open on this
      // file included.
      Quickshell.execDetached(["pkill", "-f", root.helper + " dac watch"])
    } else {
      Quickshell.execDetached(["setsid", root.helper, "dac", "watch"])
    }
    root.watching = !root.watching
  }

  function fmtHz(v) { return (Number(v) || 0).toFixed(2) }
  function fmtKhz(v) { return (Number(v) || 0).toFixed(2) }

  readonly property string headline: {
    if (!root.connected) return "NO SIGNAL"
    if (!root.active) return "STANDBY"
    if (root.lockLost) return "SYNC LOST"
    return "ON AIR"
  }
  readonly property string headlineDetail: {
    if (!root.connected) return "connect the DAC to an HDMI output"
    if (!root.active) return "press power to light the tube"
    var m = root.mode || {}
    return (m.lines || "") + " " + root.standard.toUpperCase() + " " + fmtHz(m.vfreq_hz) + " Hz"
  }
  readonly property color ledColor: {
    if (!root.connected) return root.muted
    if (!root.active) return Qt.darker(root.urgent, 1.2)
    if (root.lockLost) return root.urgent
    return root.phosphor
  }

  Process {
    id: statusProc
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        try { root.status = JSON.parse(String(text || "{}")) } catch (e) { root.status = {} }
      }
    }
  }

  Process {
    id: actionProc
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.lastNote = String(text || "").trim().split("\n").slice(-1)[0] || ""
    }
    stderr: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.errorMessage = String(text || "").trim()
    }
    onRunningChanged: if (!running) {
      root.busy = ""
      root.refresh()
      if (root.hostWidget && root.hostWidget.refresh) root.hostWidget.refresh()
    }
  }

  Process {
    id: watchProc
    command: ["pgrep", "-f", root.helper + " dac watch"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: root.watching = String(text || "").trim() !== ""
    }
  }

  Timer { interval: 2000; running: root.opened; repeat: true; onTriggered: root.refresh() }


  Timer { interval: 530; running: root.opened; repeat: true; onTriggered: root.cursorOn = !root.cursorOn }

  component Mono: Text {
    color: root.fg
    font.family: root.mono
    font.pixelSize: Style.font.body
  }

  component Key: Text {
    color: root.muted
    font.family: root.mono
    font.pixelSize: Style.font.caption
  }

  component Act: Button {
    bordered: true
    focusable: true
    foreground: root.fg
    fontFamily: root.mono
    enabled: !actionProc.running
  }

  component Row2: Row {
    property string label: ""
    property string value: ""
    property color valueColor: root.fg
    width: parent.width
    spacing: Style.space(8)
    Key { text: parent.label; width: Style.space(84); anchors.verticalCenter: parent.verticalCenter }
    Mono {
      text: parent.value
      color: parent.valueColor
      width: parent.width - Style.space(92)
      elide: Text.ElideRight
      anchors.verticalCenter: parent.verticalCenter
    }
  }

  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    owner: root.hostWidget || root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(400))
    contentHeight: panel.fittedContentHeight(Math.min(content.implicitHeight, Style.space(760)))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }

      Flickable {
        anchors.fill: parent
        contentWidth: width
        contentHeight: content.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds

        Column {
          id: content
          width: parent.width
          spacing: Style.space(10)

          // ---------------------------------------------------------- the tube
          Rectangle {
            id: tube
            width: parent.width
            height: osd.implicitHeight + Style.space(28)
            radius: Style.space(10)
            color: Qt.darker(root.bg, 1.35)
            border.width: Style.space(2)
            border.color: Qt.darker(root.fg, 2.2)
            clip: true

            // Faint phosphor glow behind the text when the tube is lit.
            Rectangle {
              anchors.centerIn: parent
              width: parent.width * 0.9
              height: parent.height * 0.8
              radius: width / 2
              color: root.active ? root.phosphor : root.fg
              opacity: root.active ? 0.06 : 0.02
            }

            Column {
              id: osd
              anchors.left: parent.left
              anchors.right: parent.right
              anchors.verticalCenter: parent.verticalCenter
              anchors.margins: Style.space(14)
              spacing: Style.space(4)

              Row {
                width: parent.width
                Mono {
                  text: "OMACRT"
                  color: root.active ? root.phosphor : root.muted
                  font.bold: true
                  font.pixelSize: Style.font.caption
                  width: parent.width - led.width - chan.width - Style.space(8)
                }
                Mono {
                  id: chan
                  text: root.active ? ("CH " + root.standard.toUpperCase()) : "CH --"
                  color: root.active ? root.phosphor : root.muted
                  font.pixelSize: Style.font.caption
                }
                Item { width: Style.space(8); height: 1 }
                Rectangle {
                  id: led
                  width: Style.space(9)
                  height: width
                  radius: width / 2
                  anchors.verticalCenter: parent.verticalCenter
                  color: root.ledColor
                  Rectangle {
                    anchors.centerIn: parent
                    width: parent.width * 2.2
                    height: width
                    radius: width / 2
                    color: parent.color
                    opacity: root.active && !root.lockLost ? 0.25 : 0
                    z: -1
                  }
                }
              }

              Row {
                width: parent.width
                spacing: 0
                Mono {
                  text: root.headline
                  color: root.active && !root.lockLost ? root.phosphor : (root.lockLost ? root.urgent : root.fg)
                  font.bold: true
                  font.pixelSize: Style.font.displayLarge
                }
                Mono {
                  text: "█"
                  color: root.active ? root.phosphor : root.fg
                  font.pixelSize: Style.font.displayLarge
                  opacity: root.cursorOn ? 0.9 : 0.0
                }
              }

              Mono {
                width: parent.width
                text: root.headlineDetail
                color: root.active ? Qt.lighter(root.phosphor, 1.15) : root.muted
                font.pixelSize: Style.font.body
                elide: Text.ElideRight
              }

              Item { width: 1; height: Style.space(4) }

              Grid {
                width: parent.width
                columns: 3
                columnSpacing: Style.space(10)
                rowSpacing: Style.space(2)
                Key { text: "H " + (root.mode && root.mode.hfreq_khz ? fmtKhz(root.mode.hfreq_khz) + " kHz" : "--") ; color: root.active ? root.phosphor : root.muted; opacity: 0.85 }
                Key { text: "V " + (root.mode && root.mode.vfreq_hz ? fmtHz(root.mode.vfreq_hz) + " Hz" : "--"); color: root.active ? root.phosphor : root.muted; opacity: 0.85 }
                Key { text: root.mode ? (root.mode.width + "x" + root.mode.height) : "--"; color: root.active ? root.phosphor : root.muted; opacity: 0.85 }
                Key { text: "DAC " + (root.dac.present ? String(root.dac.lock).toUpperCase() : "NONE"); color: root.lockLost ? root.urgent : (root.active ? root.phosphor : root.muted); opacity: 0.85 }
                Key { text: "CSYNC " + (root.csync ? root.csync.toUpperCase() : "--"); color: root.active ? root.phosphor : root.muted; opacity: 0.85 }
                Key { text: "AUDIO " + (root.audioAll ? "ALL" : (root.audioOnTv ? "TV" : "DESK")); color: root.active ? root.phosphor : root.muted; opacity: 0.85 }
              }
            }

            // Scanlines: one dark hairline every third pixel row.
            Column {
              anchors.fill: parent
              spacing: 2
              Repeater {
                model: Math.floor(tube.height / 3)
                Rectangle { width: tube.width; height: 1; color: "black"; opacity: 0.16 }
              }
            }
            // Curved glass: a soft vignette at the corners.
            Rectangle {
              anchors.fill: parent
              radius: parent.radius
              color: "transparent"
              border.width: Style.space(6)
              border.color: "black"
              opacity: 0.18
            }
          }

          // ------------------------------------------------------- the remote
          Row {
            width: parent.width
            spacing: Style.space(6)
            Act {
              width: (parent.width - Style.space(12)) / 3
              text: root.active ? "⏻  Power off" : "⏻  Power on"
              selected: root.active
              enabled: root.connected && !actionProc.running
              onClicked: root.runAction(root.active ? ["off"] : ["on"])
            }
            Act {
              width: (parent.width - Style.space(12)) / 3
              text: "NTSC 60"
              selected: root.active && root.standard === "ntsc"
              enabled: root.active && !actionProc.running
              onClicked: root.runAction(["mode", "ntsc"])
            }
            Act {
              width: (parent.width - Style.space(12)) / 3
              text: "PAL 50"
              selected: root.active && root.standard === "pal"
              enabled: root.active && !actionProc.running
              onClicked: root.runAction(["mode", "pal"])
            }
          }
          Row {
            width: parent.width
            spacing: Style.space(6)
            Act {
              width: (parent.width - Style.space(6)) / 2
              text: "⌨  Keys to the launcher"
              enabled: root.shellRunning && !actionProc.running
              onClicked: root.runAction(["focus"])
            }
            Act {
              width: (parent.width - Style.space(6)) / 2
              text: root.shellRunning ? "↻  Restart launcher" : "▶  Start launcher"
              enabled: root.active && !actionProc.running
              onClicked: root.runAction(["shell", root.shellRunning ? "restart" : "start"])
            }
          }

          // A link or a file for the tube: mpv plays it through the launcher.
          Row {
            width: parent.width
            spacing: Style.space(6)
            visible: root.shellRunning
            TextField {
              width: parent.width - Style.space(150)
              text: root.watchUrl
              foreground: root.fg
              font.family: root.mono
              onTextChanged: root.watchUrl = text
              onAccepted: if (root.watchUrl.trim()) { root.runAction(["watch", root.watchUrl.trim()]); root.watchUrl = "" }
            }
            Act {
              width: Style.space(144)
              text: "▶  Watch on the TV"
              tooltipText: "a YouTube link or a video file, played on the tube through mpv"
              enabled: root.watchUrl.trim() !== "" && !actionProc.running
              onClicked: { root.runAction(["watch", root.watchUrl.trim()]); root.watchUrl = "" }
            }
          }

          // ------------------------------------------------------- signal
          PanelSectionHeader { text: "SIGNAL"; foreground: root.fg }
          Row2 {
            label: "Output"
            value: root.conn ? (root.conn.name + "  " + (root.conn.edid_name || "") + (root.conn.rgbpi2 ? "  RGB-Pi 2" : "")) : "not found"
            valueColor: root.connected ? root.fg : root.muted
          }
          Row2 {
            visible: !!root.conn
            label: "Tube"
            value: root.conn && root.conn.leaseable
              ? (root.conn.display ? "ours: leased from the desktop, own compositor" : "leaseable, display process off")
              : "shared with the desktop (pinned windows)"
            valueColor: root.conn && root.conn.leaseable && root.conn.display ? root.fg : root.muted
          }
          Row2 {
            label: "Launcher"
            value: root.shellRunning ? ("running, pid " + root.shell.pid) : "stopped"
            valueColor: root.shellRunning ? root.fg : root.muted
          }
          Row2 {
            visible: !!root.status.playing
            label: "Playing"
            value: String(root.status.playing || "")
          }
          Row {
            width: parent.width
            spacing: Style.space(8)
            visible: root.dac.present === true
            Key { text: "Csync"; width: Style.space(84); anchors.verticalCenter: parent.verticalCenter }
            Repeater {
              model: ["and", "xor"]
              Act {
                required property string modelData
                text: modelData.toUpperCase()
                selected: root.csync === modelData
                onClicked: root.runAction(["dac", "csync", modelData])
              }
            }
            Act { text: "Reset DAC"; tooltipText: "Last resort: a reset leaves the DAC in a different colour state until its next power cycle"; onClicked: root.runAction(["dac", "reset"]) }
          }
          Toggle {
            width: parent.width
            visible: !!root.audio
            label: "Games audio on the TV"
            description: root.audioOnTv ? "launcher, RetroArch and mpv play through the DAC" : "everything stays on the desktop speakers"
            checked: root.audioOnTv
            foreground: root.fg
            onClicked: root.runAction(["audio", root.audioOnTv ? "desktop" : "crt"])
          }
          Row {
            width: parent.width
            spacing: Style.space(8)
            visible: !!root.audio
            Key { text: "TV volume"; width: Style.space(84); anchors.verticalCenter: parent.verticalCenter }
            PanelSlider {
              width: parent.width - Style.space(84) - Style.space(60) - Style.space(16)
              bar: root.bar
              minimum: 0
              maximum: 150
              step: 5
              integer: true
              value: root.volumeShown >= 0 ? root.volumeShown : root.volume
              onMoved: function(v) { root.volumeShown = Math.round(v) }
              onReleased: function(v) {
                root.volumeShown = -1
                root.runAction(["audio", "volume", String(Math.round(v))])
              }
              anchors.verticalCenter: parent.verticalCenter
            }
            Mono {
              text: (root.volumeShown >= 0 ? root.volumeShown : root.volume) + "%"
              width: Style.space(60)
              horizontalAlignment: Text.AlignRight
              anchors.verticalCenter: parent.verticalCenter
            }
          }
          Toggle {
            width: parent.width
            visible: !!root.audio && root.audioOnTv
            label: "Whole system on the TV"
            description: root.audioAll ? "the CRT is the default sink for every app" : "desktop apps keep their own output"
            checked: root.audioAll
            foreground: root.fg
            onClicked: root.runAction(["audio", root.audioAll ? "apps" : "all"])
          }
          Toggle {
            width: parent.width
            visible: root.dac.present === true
            label: "Watch the DAC"
            description: "reset the sync combiner when the picture jumps"
            checked: root.watching
            foreground: root.fg
            onClicked: root.toggleWatch()
          }

          // ------------------------------------------------------- library
          PanelSectionHeader { text: "LIBRARY"; foreground: root.fg }
          Row2 {
            label: "Games"
            value: (root.library.games || 0) + " in " + (root.library.systems || 0) + " systems, " + (root.library.videos || 0) + " videos"
          }
          Row2 {
            label: "BIOS"
            value: root.biosMissing > 0 ? (root.biosMissing + " required file(s) missing") : "all required files present"
            valueColor: root.biosMissing > 0 ? root.urgent : root.fg
          }
          Row2 {
            visible: root.missingCores.length > 0
            label: "Cores"
            value: "missing for " + root.missingCores.join(", ")
            valueColor: root.urgent
          }
          Row {
            width: parent.width
            spacing: Style.space(6)
            Act {
              text: "▤  Library"
              tooltipText: "sources, systems, cores, BIOS and unplaced folders, full screen"
              onClicked: root.openLibrary()
            }
            Act { text: "Scan library"; onClicked: root.inTerminal(root.q(root.helper) + " library scan") }
            Act { text: "Doctor"; onClicked: root.inTerminal(root.q(root.helper) + " doctor") }
          }

          // ------------------------------------------------------- footer
          Text {
            visible: root.busy !== ""
            width: parent.width
            text: "omacrt " + root.busy + " …"
            color: root.muted
            font.family: root.mono
            font.pixelSize: Style.font.caption
          }
          Text {
            visible: root.busy === "" && root.lastNote !== ""
            width: parent.width
            text: root.lastNote
            color: root.muted
            wrapMode: Text.WordWrap
            font.family: root.mono
            font.pixelSize: Style.font.caption
          }
          Text {
            visible: root.errorMessage !== ""
            width: parent.width
            text: root.errorMessage
            color: root.urgent
            wrapMode: Text.WordWrap
            font.family: root.mono
            font.pixelSize: Style.font.caption
          }
        }
      }
    }
  }
}
