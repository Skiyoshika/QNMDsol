# Troubleshooting

## Serial Device Missing

Run:

```bash
lsusb
dmesg | tail -80
ls -la /dev/serial/by-id
```

Use `/dev/serial/by-id/...` instead of `/dev/ttyUSB0` when possible.

## Permission Denied On Serial

Run:

```bash
id
sudo usermod -aG dialout "$USER"
```

Log out and log back in before retrying.

## BrainFlow Library Wrong Architecture

Run inside the container:

```bash
file /opt/brainflow/lib/libBoardController.so
```

Expected:

```text
ELF 64-bit ... ARM aarch64
```

If it reports `x86-64`, the image copied desktop libraries and must be rebuilt on arm64 or through buildx/QEMU.

## Board Opens But No Samples

Check:

- Cyton board is powered.
- Dongle and board are paired.
- Only one process owns the serial device.
- Board id is `2` for Cyton+Daisy.
