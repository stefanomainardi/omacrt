# Packaging

`PKGBUILD` builds an Arch package: the three binaries, the root part of the
lease setup with its systemd unit, the Omarchy plugins under
`/usr/share/omacrt`, and the documentation.

Building it from a checkout:

```bash
cd packaging
makepkg -si
```

The package deliberately stops at the edge of the session. Enabling the boot
time unit and installing the bar plugin into `~/.config/omarchy/plugins` are
printed as the two remaining steps: the first needs a decision about which
connector belongs to the television, the second belongs to a user and not to
the system.

`sha256sums` is `SKIP` because the source is a tag tarball from GitHub; for a
release in the AUR, replace it with the real checksum of that tarball.
