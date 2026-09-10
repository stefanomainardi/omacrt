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

`sha256sums` carries the real checksum of the tag tarball GitHub builds. It
can only be filled in after the tag is pushed, since the tarball does not
exist before then, so cutting a release is: bump the version, tag, push, then

    curl -fsSL -o /tmp/omacrt.tar.gz \
      https://github.com/stefanomainardi/omacrt/archive/refs/tags/vX.Y.Z.tar.gz
    sha256sum /tmp/omacrt.tar.gz

and commit that value here. Leaving it as `SKIP` would mean the package
builds whatever the download happens to be.
