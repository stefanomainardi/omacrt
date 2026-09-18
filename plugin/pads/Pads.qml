import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import qs.Ui

// The pads overlay: the four ports a game is given, as sockets, on the
// desktop. The same four the television draws, so the two read as one thing.
//
// What lives here and not in the bar panel: which physical pad is which
// (a pad can be made to shake), what each one will be called inside a game,
// and where the letters on its face really are. The order is the only thing
// that decides who is P1, and it is arranged from either end.
//
// An Omarchy shell plugin of kind `overlay`, its own id beside the bar
// widget: the shell mounts it on `summon` and hands the payload to `open()`.
Item {
  id: root

  property var shell: null
  property var manifest: null
  // Resolved from this plugin's own directory, never from the payload: the
  // overlay is installed as `<id>.pads` beside `<id>`, which holds
  // `bin/omacrt`. A summon from anywhere cannot name the program this runs.
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

  property var state: ({})
  property var mappings: ({})
  property string busy: ""
  property string note: ""
  property string errorMessage: ""

  readonly property var pads: (state.pads || [])
  readonly property var unlisted: (state.unlisted || [])
  readonly property int here: Number(state.connected || 0)
  readonly property bool ambiguous: state.ambiguous === true
  readonly property string file: String(state.file || "")

  // Four sockets whether they are filled or not: a person counting ports
  // should not have to count rows. Beyond the fourth a pad is remembered but
  // has no port, and says so.
  readonly property var sockets: {
    var out = []
    for (var i = 0; i < 4; i++) out.push(i < root.pads.length ? root.pads[i] : null)
    return out
  }
  readonly property var beyond: root.pads.slice(4)

  function open(payloadJson) {
    root.helper = root.ownHelper
    root.opened = true
    root.errorMessage = ""
    root.note = ""
    refresh()
    Qt.callLater(function() { keys.forceActiveFocus() })
  }

  function close() { root.opened = false }

  function dismiss() {
    root.opened = false
    if (root.shell && typeof root.shell.hide === "function")
      root.shell.hide((root.manifest && root.manifest.id) || "io.github.stefanomainardi.omacrt.pads")
  }

  function refresh() {
    if (!root.helper) return
    if (!stateProc.running) stateProc.running = true
    // The mapping needs SDL, which is slower than reading a file, so it is
    // asked for once on open and after anything that could change it.
    if (!mapProc.running) mapProc.running = true
  }

  function act(args) {
    if (!root.helper || actionProc.running) return
    root.errorMessage = ""
    root.busy = args.join(" ")
    actionProc.command = [root.helper].concat(args)
    actionProc.running = true
  }

  function parse(text, fallback) {
    try { return JSON.parse(String(text || "")) } catch (e) { return fallback }
  }

  // What SDL makes of the pad in this port, matched on the enumeration index
  // the state already carries.
  function mappingFor(pad) {
    if (!pad || pad.index === null || pad.index === undefined) return null
    var rows = root.mappings.pads || []
    for (var i = 0; i < rows.length; i++) if (rows[i].index === pad.index) return rows[i]
    return null
  }

  // One line saying where the button marked A is. This is the thing that had
  // people pressing A and going back: SDL names a face button by where it
  // sits, not by the letter beside it.
  function facesLine(pad) {
    var m = root.mappingFor(pad)
    if (!m) return ""
    if (!m.labels_known) return "letters unknown: no profile for this pad"
    if (m.swap_ab && m.swap_xy) return "letters crossed: A and B, X and Y"
    if (m.swap_ab) return "letters crossed: A and B"
    if (m.swap_xy) return "letters crossed: X and Y"
    return "letters where SDL puts them"
  }

  function howLine(pad) {
    if (!pad) return ""
    var how = pad.wireless ? "wireless" : "cable"
    if (pad.battery !== null && pad.battery !== undefined) how += "  ·  " + (pad.battery * 25) + "%"
    return how
  }

  Process {
    id: stateProc
    command: [root.helper, "pads", "--json"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.state = root.parse(text, {}) }
  }
  Process {
    id: mapProc
    command: [root.helper, "pads", "mapping", "--json"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.mappings = root.parse(text, {}) }
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

  // While the overlay is up a pad can be plugged in or switched off, and the
  // sockets should follow without being asked.
  Timer { interval: 2000; running: root.opened; repeat: true; onTriggered: root.refresh() }

  component Mono: Text {
    color: root.fg
    font.family: root.mono
    font.pixelSize: Style.font.body
    elide: Text.ElideRight
  }
  component Small: Text {
    color: root.muted
    font.family: root.mono
    font.pixelSize: Style.font.caption
    elide: Text.ElideRight
  }
  component Act: Button {
    bordered: true
    focusable: true
    foreground: root.fg
    fontFamily: root.mono
    enabled: !actionProc.running
  }
  component Line: Rectangle { width: parent.width; height: 1; color: root.line; opacity: 0.5 }

  // One port. Drawn whether it holds anything or not, because four sockets
  // that are always there is the whole idea.
  component Socket: Rectangle {
    id: sock
    property var pad: null
    property int port: 1
    property bool first: false
    property bool last: false
    readonly property bool filled: !!sock.pad
    readonly property bool live: sock.filled && sock.pad.connected === true

    width: (parent.width - Style.space(16)) / 2
    height: Style.space(150)
    radius: Style.cornerRadius
    color: sock.live ? Qt.rgba(root.accent.r, root.accent.g, root.accent.b, 0.06) : "transparent"
    border.width: Style.space(2)
    border.color: sock.live ? root.accent : root.line
    opacity: sock.filled ? 1.0 : 0.65

    Column {
      anchors.fill: parent
      anchors.margins: Style.space(14)
      spacing: Style.space(6)

      Row {
        width: parent.width
        spacing: Style.space(8)
        Text {
          text: "P" + sock.port
          color: sock.live ? root.accent : root.muted
          font.family: root.mono
          font.pixelSize: Style.font.body
          font.bold: true
          anchors.verticalCenter: parent.verticalCenter
        }
        Mono {
          width: parent.width - Style.space(110)
          text: sock.filled ? String(sock.pad.short || sock.pad.name) : "no pad"
          color: sock.live ? root.fg : root.muted
          anchors.verticalCenter: parent.verticalCenter
        }
        Small {
          // Not red: a pad that is off is a pad somebody switched off, and
          // the accent on the live socket is red in some themes. Two reds
          // meaning opposite things is worse than none.
          text: sock.filled && !sock.live ? "switched off" : ""
          anchors.verticalCenter: parent.verticalCenter
        }
      }

      Small {
        width: parent.width
        visible: sock.filled
        text: root.howLine(sock.pad) + (sock.pad && sock.pad.unit ? "  ·  " + sock.pad.unit : "")
      }
      Small {
        width: parent.width
        visible: sock.live
        // What RetroArch will call it, which is the number the launch config
        // carries and the one worth checking when a pad plays in the wrong
        // port.
        text: sock.pad && sock.pad.index !== null && sock.pad.index !== undefined
          ? ("joypad index " + sock.pad.index + "  ·  " + String(sock.pad.event || "").replace("/dev/input/", ""))
          : ""
      }
      Small {
        width: parent.width
        visible: sock.live && root.facesLine(sock.pad) !== ""
        text: root.facesLine(sock.pad)
        color: root.mappingFor(sock.pad) && (root.mappingFor(sock.pad).swap_ab || root.mappingFor(sock.pad).swap_xy)
          ? root.accent : root.muted
      }
      Small {
        width: parent.width
        visible: sock.filled && !sock.live
        text: "keeps this port until it is forgotten"
      }
      Small {
        width: parent.width
        visible: !sock.filled
        text: "the next pad written down takes it"
      }
    }

    Row {
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      anchors.margins: Style.space(12)
      spacing: Style.space(6)
      visible: sock.filled
      Act {
        text: "▲"
        enabled: !actionProc.running && !sock.first
        tooltipText: "one port towards P1"
        onClicked: root.act(["pads", "move", String(sock.port), "up"])
      }
      Act {
        text: "▼"
        enabled: !actionProc.running && !sock.last
        tooltipText: "one port away from P1"
        onClicked: root.act(["pads", "move", String(sock.port), "down"])
      }
      Act {
        text: "Identify"
        enabled: !actionProc.running && sock.live
        tooltipText: "shake this pad: the only way to tell two of one model apart"
        onClicked: root.act(["pads", "identify", String(sock.port)])
      }
      Act {
        text: "Forget"
        tooltipText: "stop remembering this pad and the port it holds"
        onClicked: root.act(["pads", "forget", String(sock.port)])
      }
    }
  }

  PanelWindow {
    id: win
    visible: root.opened
    anchors { top: true; bottom: true; left: true; right: true }
    color: "transparent"
    WlrLayershell.namespace: "omacrt-pads"
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
      width: Math.min(Style.space(860), parent.width - Style.space(80))
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

          Row {
            id: head
            width: parent.width
            spacing: Style.space(12)
            Column {
              width: parent.width - closeBtn.width - Style.space(12)
              spacing: Style.space(2)
              Text {
                text: "Pads"
                color: root.fg
                font.family: root.mono
                font.pixelSize: Style.font.title
                font.bold: true
              }
              Small {
                width: parent.width
                text: root.pads.length === 0
                  ? "no pads written down yet: plug one in and the launcher records it"
                  : (root.here + " of " + root.pads.length + " switched on  ·  the order decides who is P1, and the ports are handed out when a game starts")
              }
            }
            Act { id: closeBtn; text: "Close"; onClicked: root.dismiss(); anchors.verticalCenter: parent.verticalCenter }
          }
          Line { id: hairline }

          Flickable {
            width: parent.width
            height: Math.min(content.implicitHeight,
                             parent.height - y - footer.height - Style.space(10))
            contentWidth: width
            contentHeight: content.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds

            Column {
              id: content
              width: parent.width
              spacing: Style.space(14)

              // ------------------------------------------------------- ports
              Grid {
                width: parent.width
                columns: 2
                columnSpacing: Style.space(16)
                rowSpacing: Style.space(16)
                Repeater {
                  model: 4
                  Socket {
                    required property int index
                    port: index + 1
                    pad: root.sockets[index]
                    first: index === 0
                    last: index >= root.pads.length - 1
                  }
                }
              }

              // Pads remembered past the fourth port: they hold no port until
              // one above them is forgotten or moved down.
              Column {
                width: parent.width
                spacing: Style.space(6)
                visible: root.beyond.length > 0
                PanelSectionHeader { text: "WAITING FOR A PORT"; foreground: root.fg }
                Repeater {
                  model: root.beyond
                  Row {
                    required property var modelData
                    width: parent.width
                    spacing: Style.space(8)
                    Mono {
                      width: parent.width - Style.space(220)
                      text: String(modelData.short || modelData.name)
                      color: root.muted
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Small {
                      text: root.howLine(modelData)
                      width: Style.space(120)
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Act {
                      text: "▲"
                      tooltipText: "one port towards P1"
                      onClicked: root.act(["pads", "move", String(modelData.port), "up"])
                    }
                    Act {
                      text: "Forget"
                      onClicked: root.act(["pads", "forget", String(modelData.port)])
                    }
                  }
                }
              }

              // A pad that is plugged in and plays, but that nothing has
              // written down yet. It goes after the listed ones at the next
              // launch, and joins the list the next time the launcher runs.
              Column {
                width: parent.width
                spacing: Style.space(6)
                visible: root.unlisted.length > 0
                PanelSectionHeader { text: "NOT WRITTEN DOWN"; foreground: root.fg }
                Repeater {
                  model: root.unlisted
                  Row {
                    required property var modelData
                    width: parent.width
                    spacing: Style.space(8)
                    Mono {
                      width: parent.width - Style.space(260)
                      text: String(modelData.short || modelData.name)
                      color: root.accent
                      anchors.verticalCenter: parent.verticalCenter
                    }
                    Small {
                      text: String(modelData.unit || "")
                      width: Style.space(240)
                      anchors.verticalCenter: parent.verticalCenter
                    }
                  }
                }
                Small {
                  width: parent.width
                  text: "These play already, after the listed ones. They take a port of their own once the launcher has seen them."
                  wrapMode: Text.WordWrap
                }
              }

              Column {
                width: parent.width
                spacing: Style.space(4)
                visible: root.ambiguous
                Line {}
                Small {
                  width: parent.width
                  wrapMode: Text.WordWrap
                  color: root.urgent
                  text: "Two pads of one model report no serial. Nothing can tell them apart, so the order between those two is whatever the kernel numbered them, not what is here. Identify shakes one of them; the other is the other."
                }
              }
            }
          }

          Column {
            id: footer
            width: parent.width
            spacing: Style.space(6)
            Line {}
            Row {
              width: parent.width
              spacing: Style.space(8)
              Act { text: "Reload"; onClicked: root.refresh() }
              Text {
                width: parent.width - Style.space(120)
                anchors.verticalCenter: parent.verticalCenter
                text: root.busy !== "" ? ("omacrt " + root.busy + " …")
                    : (root.errorMessage !== "" ? root.errorMessage
                    : (root.note !== "" ? root.note : root.file))
                color: root.errorMessage !== "" ? root.urgent : root.muted
                font.family: root.mono
                font.pixelSize: Style.font.caption
                elide: Text.ElideMiddle
              }
            }
          }
        }
      }
    }
  }
}
