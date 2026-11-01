use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::process::ExitCode;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, BusType, InputId, KeyCode, KeyEvent};
use razer_fn_super_core::Key;

const HIDRAW: &str = "/dev/input/by-id/usb-Razer_Razer_BlackWidow_Elite-if01-hidraw";
const DEVICE_NAME: &str = "Razer FN as Super";
const SUPER: KeyCode = KeyCode::KEY_LEFTMETA;
const REPORT_CAPACITY: usize = 64;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("razer-fn-super: {failure}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let hidraw: File =
        File::open(HIDRAW).map_err(|error: io::Error| format!("opening {HIDRAW}: {error}"))?;
    let mut super_key: VirtualDevice = create_super_key()
        .map_err(|error: io::Error| format!("creating uinput device: {error}"))?;
    translate(hidraw, |key: Key| emit(&mut super_key, key))
        .or_else(accept_disconnect)
        .map_err(|error: io::Error| format!("translating reports: {error}"))
}

fn create_super_key() -> io::Result<VirtualDevice> {
    let keys: AttributeSet<KeyCode> = AttributeSet::from_iter([SUPER]);
    VirtualDevice::builder()?
        .name(DEVICE_NAME)
        .input_id(InputId::new(BusType::BUS_USB, 1, 1, 1))
        .with_keys(&keys)?
        .build()
}

fn emit(device: &mut VirtualDevice, key: Key) -> io::Result<()> {
    let value: i32 = match key {
        Key::Held => 1,
        Key::Released => 0,
    };
    device.emit(&[*KeyEvent::new(SUPER, value)])
}

fn accept_disconnect(error: io::Error) -> io::Result<()> {
    match error.raw_os_error() {
        Some(libc::ENODEV | libc::EIO) => Ok(()),
        Some(_) | None => Err(error),
    }
}

fn translate(
    mut reports: impl Read,
    mut emit: impl FnMut(Key) -> io::Result<()>,
) -> io::Result<()> {
    let mut report: [u8; REPORT_CAPACITY] = [0; REPORT_CAPACITY];
    let mut key: Key = Key::Released;
    let outcome: io::Result<()> = loop {
        let length: usize = match reports.read(&mut report) {
            Ok(0) => break Ok(()),
            Ok(length) => length,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => break Err(error),
        };
        if let Some(next) = key.after_report(&report[..length]) {
            key = next;
            if let Err(error) = emit(next) {
                break Err(error);
            }
        }
    };
    let release: io::Result<()> = key.after_end().map_or(Ok(()), emit);
    outcome.and(release)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::{self, ErrorKind, Read};

    use razer_fn_super_core::Key;

    use super::{accept_disconnect, translate};

    const PRESS: &[u8] = &[0x04, 0x01];
    const RELEASE: &[u8] = &[0x04, 0x00];

    enum Step {
        Report(&'static [u8]),
        Fail(io::Error),
    }

    struct Script(VecDeque<Step>);

    impl Script {
        fn of(steps: impl IntoIterator<Item = Step>) -> Self {
            Self(steps.into_iter().collect())
        }
    }

    impl Read for Script {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            match self.0.pop_front() {
                None => Ok(0),
                Some(Step::Fail(error)) => Err(error),
                Some(Step::Report(report)) => {
                    buffer[..report.len()].copy_from_slice(report);
                    Ok(report.len())
                }
            }
        }
    }

    fn emitted(script: Script) -> (Vec<Key>, io::Result<()>) {
        let mut keys: Vec<Key> = Vec::new();
        let outcome: io::Result<()> = translate(script, |key: Key| {
            keys.push(key);
            Ok(())
        });
        (keys, outcome)
    }

    #[test]
    fn no_reports_emit_nothing() {
        let (keys, outcome) = emitted(Script::of([]));
        assert_eq!(keys, []);
        assert!(outcome.is_ok());
    }

    #[test]
    fn press_then_release_emits_both_edges_once() {
        let (keys, _) = emitted(Script::of([Step::Report(PRESS), Step::Report(RELEASE)]));
        assert_eq!(keys, [Key::Held, Key::Released]);
    }

    #[test]
    fn end_of_stream_while_held_releases_the_key() {
        let (keys, outcome) = emitted(Script::of([Step::Report(PRESS)]));
        assert_eq!(keys, [Key::Held, Key::Released]);
        assert!(outcome.is_ok());
    }

    #[test]
    fn read_error_while_held_releases_the_key_and_reports_the_error() {
        let (keys, outcome) = emitted(Script::of([
            Step::Report(PRESS),
            Step::Fail(io::Error::from_raw_os_error(libc::EACCES)),
        ]));
        assert_eq!(keys, [Key::Held, Key::Released]);
        assert_eq!(
            outcome.map_err(|error| error.raw_os_error()),
            Err(Some(libc::EACCES))
        );
    }

    #[test]
    fn read_error_while_released_emits_nothing() {
        let (keys, outcome) = emitted(Script::of([Step::Fail(io::Error::from_raw_os_error(
            libc::EIO,
        ))]));
        assert_eq!(keys, []);
        assert!(outcome.is_err());
    }

    #[test]
    fn interrupted_read_keeps_translating() {
        let (keys, outcome) = emitted(Script::of([
            Step::Fail(ErrorKind::Interrupted.into()),
            Step::Report(PRESS),
            Step::Report(RELEASE),
        ]));
        assert_eq!(keys, [Key::Held, Key::Released]);
        assert!(outcome.is_ok());
    }

    #[test]
    fn failed_release_is_reported() {
        let script: Script = Script::of([Step::Report(PRESS)]);
        let outcome: io::Result<()> = translate(script, |key: Key| match key {
            Key::Held => Ok(()),
            Key::Released => Err(io::Error::from_raw_os_error(libc::EBADF)),
        });
        assert_eq!(
            outcome.map_err(|error| error.raw_os_error()),
            Err(Some(libc::EBADF))
        );
    }

    #[test]
    fn enodev_is_a_clean_disconnect() {
        assert!(accept_disconnect(io::Error::from_raw_os_error(libc::ENODEV)).is_ok());
    }

    #[test]
    fn eio_is_a_clean_disconnect() {
        assert!(accept_disconnect(io::Error::from_raw_os_error(libc::EIO)).is_ok());
    }

    #[test]
    fn other_errors_are_not_a_disconnect() {
        assert!(accept_disconnect(io::Error::from_raw_os_error(libc::EACCES)).is_err());
    }
}
