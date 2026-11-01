use std::fmt;
use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::process::ExitCode;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, BusType, EventType, InputEvent, InputId, KeyCode};
use razer_fn_super_core::Key;

const HIDRAW: &str = "/dev/input/by-id/usb-Razer_Razer_BlackWidow_Elite-if01-hidraw";
const DEVICE_NAME: &str = "Razer FN as Super";
const REPORT_CAPACITY: usize = 64;

enum Failure {
    OpenHidraw(io::Error),
    CreateUinput(io::Error),
    Translate(io::Error),
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenHidraw(error) => write!(formatter, "opening {HIDRAW}: {error}"),
            Self::CreateUinput(error) => write!(formatter, "creating uinput device: {error}"),
            Self::Translate(error) => write!(formatter, "translating reports: {error}"),
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("razer-fn-super: {failure}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Failure> {
    let mut hidraw: File = File::open(HIDRAW).map_err(Failure::OpenHidraw)?;
    let mut super_key: VirtualDevice = create_super_key().map_err(Failure::CreateUinput)?;
    let outcome: io::Result<()> = translate(&mut hidraw, &mut |key: Key| emit(&mut super_key, key));
    outcome
        .or_else(accept_disconnect)
        .map_err(Failure::Translate)
}

fn create_super_key() -> io::Result<VirtualDevice> {
    let keys: AttributeSet<KeyCode> = AttributeSet::from_iter([KeyCode::KEY_LEFTMETA]);
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
    device.emit(&[InputEvent::new(
        EventType::KEY.0,
        KeyCode::KEY_LEFTMETA.0,
        value,
    )])
}

fn accept_disconnect(error: io::Error) -> io::Result<()> {
    match error.raw_os_error() {
        Some(libc::ENODEV | libc::EIO) => Ok(()),
        Some(_) | None => Err(error),
    }
}

fn translate(
    reports: &mut impl Read,
    emit: &mut impl FnMut(Key) -> io::Result<()>,
) -> io::Result<()> {
    let mut key: Key = Key::Released;
    let outcome: io::Result<()> = forward(reports, emit, &mut key);
    let release: io::Result<()> = match key {
        Key::Held => emit(Key::Released),
        Key::Released => Ok(()),
    };
    outcome.and(release)
}

fn forward(
    reports: &mut impl Read,
    emit: &mut impl FnMut(Key) -> io::Result<()>,
    key: &mut Key,
) -> io::Result<()> {
    let mut report: [u8; REPORT_CAPACITY] = [0; REPORT_CAPACITY];
    loop {
        let length: usize = match reports.read(&mut report) {
            Ok(0) => return Ok(()),
            Ok(length) => length,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if let Some(next) = key.after_report(&report[..length]) {
            *key = next;
            emit(next)?;
        }
    }
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

    fn emitted(mut script: Script) -> (Vec<Key>, io::Result<()>) {
        let mut keys: Vec<Key> = Vec::new();
        let outcome: io::Result<()> = translate(&mut script, &mut |key: Key| {
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
        let mut script: Script = Script::of([Step::Report(PRESS)]);
        let outcome: io::Result<()> = translate(&mut script, &mut |key: Key| match key {
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
