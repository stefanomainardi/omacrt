# Packaging

`PKGBUILD` builds an Arch package: the three binaries, the root part of the
lease setup with its systemd unit, the Omarchy plugins under
`/usr/share/omacrt`, and the documentation.

Building it from a checkout:

```bash
cd packaging
makepkg -si
```

After installation, follow the printed instructions to enable the boot time
unit and install the plugins into `~/.config/omarchy/plugins`. These steps
require choosing the television's connector and the user account that runs
the desktop.

`sha256sums` carries the real checksum of the tag tarball GitHub builds. It
can only be filled in after the tag is pushed, since the tarball does not
exist before then, so cutting a release is: bump the version, tag, push, then

    curl -fsSL -o /tmp/omacrt.tar.gz \
      https://github.com/stefanomainardi/omacrt/archive/refs/tags/vX.Y.Z.tar.gz
    sha256sum /tmp/omacrt.tar.gz

and commit that value here. Leaving it as `SKIP` would mean the package
builds whatever the download happens to be.
