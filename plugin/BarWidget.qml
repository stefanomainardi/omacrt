import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// OmaCRT bar widget: a television glyph that knows whether a 15 kHz
// tube is on the air. Everything real happens in the `omacrt` binary
// shipped next to this file; the widget renders its JSON and opens the panel.
BarWidget {
  id: root
  moduleName: "io.github.stefanomainardi.omacrt"

  property var status: ({})
  readonly property bool connected: !!(status.connector && status.connector.connected)
  readonly property bool active: status.active === true
  // Only a lost lock while the tube is on the air is news; standby reads as lost too.
  readonly property bool lockLost: root.active && !!(status.dac && status.dac.present && status.dac.lock === "lost")
  readonly property string lines: (status.mode && status.mode.lines) ? status.mode.lines : ""
  readonly property bool hideWhenAbsent: setting("hideWhenAbsent", false) === true
  readonly property int refreshIntervalSec: Math.max(2, Math.min(60, Number(setting("refreshIntervalSec", 5)) || 5))
  readonly property string helper: Qt.resolvedUrl("bin/omacrt").toString().replace("file://", "")
  readonly property bool opened: panelLoader.item ? panelLoader.item.opened === true : false
  readonly property bool popoutSwitchClosing: panelLoader.item
    ? panelLoader.item.popoutSwitchClosing === true
    : false

  function refresh() {
    if (!statusProc.running) statusProc.running = true
    if (panelLoader.item && panelLoader.item.refresh) panelLoader.item.refresh()
  }

  function open() { if (panelLoader.item) panelLoader.item.open() }
  function close() { if (panelLoader.item) panelLoader.item.close() }
  function toggle() { if (panelLoader.item) panelLoader.item.toggle() }
  function closeForPopoutSwitch() { if (panelLoader.item) panelLoader.item.closeForPopoutSwitch() }

  function act(args) {
    if (panelLoader.item) panelLoader.item.runAction(args)
  }

  function library() { if (panelLoader.item) panelLoader.item.openLibrary() }

  function injectPanel() {
    if (!panelLoader.item) return
    panelLoader.item.bar = root.bar
    panelLoader.item.settings = root.settings
    panelLoader.item.anchorItem = button
    panelLoader.item.hostWidget = root
    panelLoader.item.helper = root.helper
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight
  visible: !root.hideWhenAbsent || root.connected

  onBarChanged: injectPanel()
  onSettingsChanged: injectPanel()
  Component.onCompleted: refresh()

  Timer {
    interval: root.refreshIntervalSec * 1000
    running: true
    repeat: true
    onTriggered: root.refresh()
  }

  Process {
    id: statusProc
    command: [root.helper, "status", "--json"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        try {
          root.status = JSON.parse(String(text || "{}"))
        } catch (e) {
          root.status = {}
        }
        if (panelLoader.item) panelLoader.item.status = root.status
      }
    }
  }

  Loader {
    id: panelLoader
    active: true
    source: Qt.resolvedUrl("Panel.qml")
    visible: false
    onLoaded: {
      root.injectPanel()
      Qt.callLater(root.injectPanel)
    }
  }

  IpcHandler {
    target: root.moduleName
    function refresh(): void { root.refresh() }
    function open(): void { root.open() }
    function close(): void { root.close() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
    function toggle(): void { root.toggle() }
    function power(): void { root.act(["toggle"]) }
    function on(): void { root.act(["on"]) }
    function off(): void { root.act(["off"]) }
    function ntsc(): void { root.act(["mode", "ntsc"]) }
    function pal(): void { root.act(["mode", "pal"]) }
    function focus(): void { root.act(["focus"]) }
    function library(): void { root.library() }
    function restart(): void { root.act(["shell", "restart"]) }
    function audio(where: string): void { root.act(["audio", where]) }
    function csync(mode: string): void { root.act(["dac", "csync", mode]) }
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    // nf-md-television, with the line standard once the tube is on the air.
    text: root.active ? ("󰔂 " + root.lines) : "󰔂"
    tooltipText: root.active
      ? ("OmaCRT on the air, " + root.lines)
      : (root.connected ? "OmaCRT in standby" : "No CRT DAC connected")
    dimmed: !root.connected
    active: root.lockLost
    onPressed: function(buttonCode) {
      if (buttonCode === Qt.LeftButton) root.toggle()
      if (buttonCode === Qt.MiddleButton) root.act(["toggle"])
      if (buttonCode === Qt.RightButton) root.refresh()
    }
  }
}
