# razer-fn-super

Makes the FN key on a Razer BlackWidow Elite act as Super (left Meta) on Linux.

The keyboard reports FN as vendor report 4 on USB interface 1. Its HID descriptor
puts that report on an unmapped usage, so `hid-generic` drops it and no input event
ever exists for FN. This daemon reads the interface's hidraw node and replays FN as
`KEY_LEFTMETA` on a uinput device named `Razer FN as Super`. hidraw is a copy of the
stream, so the kernel's own key handling is untouched.

## Requirements

A Razer BlackWidow Elite (USB `1532:0228`), Linux with systemd and udev, and a Rust
toolchain (1.95 or later).

## Install

    ./install.sh

Builds the release binary and installs it to `/usr/local/bin/razer-fn-super`, with
the systemd unit and udev rule from `system/`. Udev starts the service when the
keyboard's interface 1 hidraw node appears. The service exits when the keyboard goes
away and is started again on the next plug-in.

The uninstall steps are in reverse: stop the service, then remove those three files.

## Invariants

| Invariant | Force | Enforced by |
|---|---|---|
| A report is a read whose first byte is `0x04`; FN is held when `0x01` appears anywhere in the rest of that read | The keyboard lists held keys in any slot, so position carries no meaning | `core/src/lib.rs` tests |
| Only a change of FN state emits an event | Reports repeat while keys are held; a repeated press would be a false auto-repeat | `core/src/lib.rs` tests |
| A held key is released on every exit that runs to completion | Super left stuck down would capture all later typing | `translate` tests in `src/main.rs` |
| ENODEV, EIO and end of stream exit 0; anything else exits 1 | `Restart=on-failure` must not loop on an unplugged keyboard but must retry real faults | `accept_disconnect` tests in `src/main.rs` |
| SIGTERM and SIGKILL rely on the kernel | Closing the uinput device releases its keys, so no signal handler is needed | Kernel behaviour |
| The core has no dependencies and no `std` | Report logic stays testable with no I/O in its tree | `#![no_std]`, `hex-lint` |

## Development

    scripts/check.sh

Runs the no-comments gate, rustfmt, clippy, the tests and `hex-lint`.

Mutation testing with `cargo mutants --workspace` leaves four survivors in the I/O
shell (`main`, `run`, `emit`, `Failure::fmt`). They need a real hidraw and uinput
device, so no unit test reaches them.

There is no generated view of the structure: the project is two crates and a two-state enum, so
reading the code is faster. Add one if that stops being true.

## License

MIT OR Apache-2.0.
