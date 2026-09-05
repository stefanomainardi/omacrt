#!/usr/bin/env bash
# Throwaway Omarchy VM for testing the linux-crt kernel package and the Limine
# boot entries without touching the host. QEMU/KVM, UEFI (OVMF), qcow2 disk.
#
# Usage:
#   scripts/vm.sh create            download the ISO, create disk and UEFI vars
#   scripts/vm.sh install           boot from the ISO (first run, install Omarchy)
#   scripts/vm.sh run               boot the installed system
#   scripts/vm.sh snapshot NAME     save a disk snapshot (before installing a kernel)
#   scripts/vm.sh restore NAME      go back to a snapshot
#   scripts/vm.sh ssh [CMD]         ssh into the guest (port 2222)
#
# Needs: qemu-desktop, edk2-ovmf (pacman -S qemu-desktop edk2-ovmf).
# The user must be able to open /dev/kvm (group kvm, or udev rule).
set -euo pipefail

VM_DIR="${OMARCHY_CRT_VM_DIR:-$HOME/.local/share/omarchy-crt/vm}"
ISO_URL="${OMARCHY_ISO_URL:-https://iso.omarchy.org/omarchy-4.0.1.iso}"
ISO="$VM_DIR/$(basename "$ISO_URL")"
DISK="$VM_DIR/omarchy.qcow2"
DISK_SIZE="${DISK_SIZE:-48G}"
RAM="${RAM:-8G}"
CPUS="${CPUS:-8}"
OVMF_CODE=/usr/share/edk2/x64/OVMF_CODE.4m.fd
OVMF_VARS_SRC=/usr/share/edk2/x64/OVMF_VARS.4m.fd
OVMF_VARS="$VM_DIR/OVMF_VARS.fd"
SSH_PORT="${SSH_PORT:-2222}"

need() {
  command -v "$1" >/dev/null 2>&1 || { echo "missing: $1 ($2)" >&2; exit 1; }
}

qemu_common() {
  qemu-system-x86_64 \
    -enable-kvm -machine q35,accel=kvm -cpu host -smp "$CPUS" -m "$RAM" \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive if=pflash,format=raw,file="$OVMF_VARS" \
    -drive file="$DISK",if=virtio,format=qcow2,discard=unmap \
    -device virtio-vga-gl -display gtk,gl=on \
    -device virtio-keyboard-pci -device virtio-mouse-pci \
    -audiodev pipewire,id=snd0 -device intel-hda -device hda-output,audiodev=snd0 \
    -netdev user,id=net0,hostfwd=tcp::"$SSH_PORT"-:22 -device virtio-net-pci,netdev=net0 \
    -rtc base=utc \
    "$@"
}

case "${1:-}" in
  create)
    need qemu-system-x86_64 "pacman -S qemu-desktop"
    need qemu-img "pacman -S qemu-desktop"
    [ -f "$OVMF_CODE" ] || { echo "missing $OVMF_CODE (pacman -S edk2-ovmf)" >&2; exit 1; }
    mkdir -p "$VM_DIR"
    [ -f "$ISO" ] || curl -L --fail -o "$ISO" "$ISO_URL"
    [ -f "$DISK" ] || qemu-img create -f qcow2 "$DISK" "$DISK_SIZE"
    [ -f "$OVMF_VARS" ] || cp "$OVMF_VARS_SRC" "$OVMF_VARS"
    echo "ready in $VM_DIR"
    ;;
  install)
    qemu_common -cdrom "$ISO" -boot d
    ;;
  run)
    qemu_common
    ;;
  snapshot)
    qemu-img snapshot -c "${2:?name}" "$DISK"
    qemu-img snapshot -l "$DISK"
    ;;
  restore)
    qemu-img snapshot -a "${2:?name}" "$DISK"
    ;;
  ssh)
    shift
    ssh -p "$SSH_PORT" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null localhost "$@"
    ;;
  *)
    sed -n '2,13p' "$0"
    exit 1
    ;;
esac
