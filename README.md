# razer-fn-super

Makes the FN key on a Razer BlackWidow Elite act as Super (left Meta) on Linux, and
gives its media keys key codes of their own.

The keyboard reports FN, and in driver mode its media keys, as vendor report 4 on USB
interface 1. Its HID descriptor puts that report on an unmapped usage, so `hid-generic`
drops it and no input event ever exists for those keys. This daemon reads the
interface's hidraw node and replays them on a uinput device named
`Razer FN and media keys`. hidraw is a copy of the stream, so the kernel's own key
handling is untouched.

| Key | Vendor code | Emitted as |
|---|---|---|
| FN | `0x01` | `KEY_LEFTMETA` |
| Previous | `0x54` | `KEY_F13` |
| Play/Pause | `0x55` | `KEY_F14` |
| Next | `0x53` | `KEY_F15` |
| Mute | `0x52` | `KEY_MUTE` |
| Volume wheel | `0x50`, `0x51` | nothing |

Previous, Play/Pause and Next are spare keys so that their meaning is bound in the
compositor, not here. The wheel is ignored because the one this was written for is
physically broken and jitters.

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
| A report is a read whose first byte is `0x04`; a key is held when its code appears anywhere in the rest of that read; unknown codes and the wheel's hold nothing | The keyboard lists held keys in any slot, so position carries no meaning | `core/src/lib.rs` tests |
| Only a change of a key's state emits an event, and only for that key | Reports repeat while keys are held; a repeated press would be a false auto-repeat | `edges_to` tests in `core/src/lib.rs`, `translate` tests in `src/main.rs` |
| Every held key is released on every exit that runs to completion | Super left stuck down would capture all later typing | `after_end` tests in `core/src/lib.rs`, `translate` tests in `src/main.rs` |
| Every emitted key code is registered on the uinput device | uinput silently drops an unregistered code | `create_keys` builds its set from `key_code` over `VendorKey::ALL` |
| ENODEV, EIO and end of stream exit 0; anything else exits 1 | `Restart=on-failure` must not loop on an unplugged keyboard but must retry real faults | `accept_disconnect` tests in `src/main.rs` |
| SIGTERM and SIGKILL have no handler | Assumed: closing the uinput device releases its keys | Nothing; not tested |
| The core has no dependencies and no `std` | Report logic stays testable with no I/O in its tree | `#![no_std]`, `hex-lint` |

## Development

    scripts/check.sh

Runs the no-comments gate, rustfmt, clippy, the tests and `hex-lint`.

Mutation testing with `cargo mutants --workspace` leaves three survivors in the I/O
shell (`main`, `run`, `emit`). They need a real hidraw and uinput
device, so no unit test reaches them.

There is no generated view of the structure: the project is two crates and a two-state enum, so
reading the code is faster. Add one if that stops being true.

## License

MIT OR Apache-2.0.
