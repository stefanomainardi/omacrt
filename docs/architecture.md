# The architecture, as a flowchart

The drawn version is in the README. This is the same thing with every process
and every channel between them named, for the parts a picture cannot hold.

```mermaid
flowchart LR
  subgraph desktop["Omarchy desktop (Hyprland)"]
    bar["Bar plugin<br/>Quickshell panel + library overlay"]
    cli["omarchy-crt<br/>CLI"]
    cliamp["cliamp --daemon<br/>music engine"]
    bar --> cli
  end

  subgraph tube["The tube (leased DRM connector)"]
    display["omarchy-crt-display<br/>own Wayland compositor (smithay)<br/>sets 15 kHz modelines through DRM"]
    shell["omarchy-crt-shell<br/>launcher, 320x240"]
    ra["RetroArch"]
    mpv["mpv"]
    display --- shell
    display --- ra
    display --- mpv
  end

  cli -- "on / off / mode<br/>lease + hotkeys" --> display
  cli -- "control pipe:<br/>keys, type, watch" --> shell
  shell -- "launch, pause menu<br/>(hotkeys pressed by the compositor)" --> ra
  shell -- "JSON IPC" --> mpv
  shell -- "Unix socket IPC<br/>status, spectrum, lyrics" --> cliamp
  display -- "HDMI, 3520x240 @ 15.73 kHz<br/>+ audio" --> dac["RGB-Pi 2 DAC<br/>csync over I2C"]
  dac -- "RGB SCART" --> tv["CRT television"]
  shell -. "covers, radio directory,<br/>YouTube via yt-dlp" .-> net["Internet"]
```

Read it against [`15khz.md`](15khz.md), which says why the tube has a
compositor of its own rather than being one more output of the desktop.
