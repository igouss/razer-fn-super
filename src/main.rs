use std::ffi::CString;
use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, BusType, InputId, KeyCode, KeyEvent};
use hidapi::{HidApi, HidDevice, HidError};
use razer_fn_super_core::{Command, Edge, FEATURE_REPORT_LEN, Held, KeyState, VendorKey};

const HIDRAW: &str = "/dev/input/by-id/usb-Razer_Razer_BlackWidow_Elite-if01-hidraw";
const DEVICE_NAME: &str = "Razer FN and media keys";
const REPORT_CAPACITY: usize = 64;
const COMMAND_WAIT: Duration = Duration::from_millis(10);

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
    enter_driver_mode()?;
    let hidraw: File =
        File::open(HIDRAW).map_err(|error: io::Error| format!("opening {HIDRAW}: {error}"))?;
    let mut keys: VirtualDevice =
        create_keys().map_err(|error: io::Error| format!("creating uinput device: {error}"))?;
    translate(hidraw, |edge: Edge| emit(&mut keys, edge))
        .or_else(accept_disconnect)
        .map_err(|error: io::Error| format!("translating reports: {error}"))
}

fn enter_driver_mode() -> Result<(), String> {
    let path: CString = CString::new(HIDRAW).map_err(|error| format!("{HIDRAW}: {error}"))?;
    HidApi::disable_device_discovery();
    let device: HidDevice = HidApi::new()
        .and_then(|api: HidApi| api.open_path(&path))
        .map_err(|error: HidError| format!("opening {HIDRAW} for commands: {error}"))?;
    Command::HANDSHAKE
        .into_iter()
        .try_for_each(|command: Command| {
            let response: Vec<u8> = exchange(&device, command)
                .map_err(|error: HidError| format!("sending {command:?}: {error}"))?;
            command
                .check(&response)
                .map_err(|refusal| format!("{command:?} refused: {refusal}"))
        })
}

fn exchange(device: &HidDevice, command: Command) -> Result<Vec<u8>, HidError> {
    device.send_feature_report(&command.request())?;
    thread::sleep(COMMAND_WAIT);
    let mut response: [u8; FEATURE_REPORT_LEN] = [0; FEATURE_REPORT_LEN];
    let length: usize = device.get_feature_report(&mut response)?;
    Ok(response[..length].to_vec())
}

const fn key_code(key: VendorKey) -> KeyCode {
    match key {
        VendorKey::Fn => KeyCode::KEY_LEFTMETA,
        VendorKey::Previous => KeyCode::KEY_F13,
        VendorKey::PlayPause => KeyCode::KEY_F14,
        VendorKey::Next => KeyCode::KEY_F15,
        VendorKey::Mute => KeyCode::KEY_MUTE,
    }
}

fn create_keys() -> io::Result<VirtualDevice> {
    let keys: AttributeSet<KeyCode> = VendorKey::ALL.into_iter().map(key_code).collect();
    VirtualDevice::builder()?
        .name(DEVICE_NAME)
        .input_id(InputId::new(BusType::BUS_USB, 1, 1, 1))
        .with_keys(&keys)?
        .build()
}

fn emit(device: &mut VirtualDevice, edge: Edge) -> io::Result<()> {
    let value: i32 = match edge.state {
        KeyState::Held => 1,
        KeyState::Released => 0,
    };
    device.emit(&[*KeyEvent::new(key_code(edge.key), value)])
}

fn accept_disconnect(error: io::Error) -> io::Result<()> {
    match error.raw_os_error() {
        Some(libc::ENODEV | libc::EIO) => Ok(()),
        Some(_) | None => Err(error),
    }
}

fn translate(
    mut reports: impl Read,
    mut emit: impl FnMut(Edge) -> io::Result<()>,
) -> io::Result<()> {
    let mut report: [u8; REPORT_CAPACITY] = [0; REPORT_CAPACITY];
    let mut held: Held = Held::NONE;
    let outcome: io::Result<()> = loop {
        let length: usize = match reports.read(&mut report) {
            Ok(0) => break Ok(()),
            Ok(length) => length,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => break Err(error),
        };
        if let Some(next) = Held::from_report(&report[..length]) {
            let previous: Held = held;
            held = next;
            if let Err(error) = previous.edges_to(next).try_for_each(&mut emit) {
                break Err(error);
            }
        }
    };
    let release: io::Result<()> = held.after_end().try_for_each(&mut emit);
    outcome.and(release)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::{self, ErrorKind, Read};

    use razer_fn_super_core::{Edge, KeyState, VendorKey};

    use super::{accept_disconnect, translate};

    const FN: &[u8] = &[0x04, 0x01];
    const FN_AND_PLAY: &[u8] = &[0x04, 0x01, 0x55];
    const NOTHING: &[u8] = &[0x04, 0x00];
    const OTHER_REPORT: &[u8] = &[0x01, 0x00, 0x45];

    const fn press(key: VendorKey) -> Edge {
        Edge {
            key,
            state: KeyState::Held,
        }
    }

    const fn release(key: VendorKey) -> Edge {
        Edge {
            key,
            state: KeyState::Released,
        }
    }

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

    fn emitted(script: Script) -> (Vec<Edge>, io::Result<()>) {
        let mut edges: Vec<Edge> = Vec::new();
        let outcome: io::Result<()> = translate(script, |edge: Edge| {
            edges.push(edge);
            Ok(())
        });
        (edges, outcome)
    }

    #[test]
    fn no_reports_emit_nothing() {
        let (edges, outcome) = emitted(Script::of([]));
        assert_eq!(edges, []);
        assert!(outcome.is_ok());
    }

    #[test]
    fn press_then_release_emits_both_edges_once() {
        let (edges, _) = emitted(Script::of([Step::Report(FN), Step::Report(NOTHING)]));
        assert_eq!(edges, [press(VendorKey::Fn), release(VendorKey::Fn)]);
    }

    #[test]
    fn repeated_report_emits_no_second_press() {
        let (edges, _) = emitted(Script::of([
            Step::Report(FN),
            Step::Report(FN),
            Step::Report(NOTHING),
        ]));
        assert_eq!(edges, [press(VendorKey::Fn), release(VendorKey::Fn)]);
    }

    #[test]
    fn other_report_ids_emit_nothing() {
        let (edges, _) = emitted(Script::of([
            Step::Report(FN),
            Step::Report(OTHER_REPORT),
            Step::Report(NOTHING),
        ]));
        assert_eq!(edges, [press(VendorKey::Fn), release(VendorKey::Fn)]);
    }

    #[test]
    fn second_key_while_one_is_held_emits_only_its_edges() {
        let (edges, _) = emitted(Script::of([
            Step::Report(FN),
            Step::Report(FN_AND_PLAY),
            Step::Report(FN),
            Step::Report(NOTHING),
        ]));
        assert_eq!(
            edges,
            [
                press(VendorKey::Fn),
                press(VendorKey::PlayPause),
                release(VendorKey::PlayPause),
                release(VendorKey::Fn),
            ]
        );
    }

    #[test]
    fn end_of_stream_while_held_releases_the_key() {
        let (edges, outcome) = emitted(Script::of([Step::Report(FN)]));
        assert_eq!(edges, [press(VendorKey::Fn), release(VendorKey::Fn)]);
        assert!(outcome.is_ok());
    }

    #[test]
    fn end_of_stream_with_two_held_releases_both() {
        let (edges, outcome) = emitted(Script::of([Step::Report(FN_AND_PLAY)]));
        assert_eq!(
            edges,
            [
                press(VendorKey::Fn),
                press(VendorKey::PlayPause),
                release(VendorKey::Fn),
                release(VendorKey::PlayPause),
            ]
        );
        assert!(outcome.is_ok());
    }

    #[test]
    fn read_error_while_held_releases_the_key_and_reports_the_error() {
        let (edges, outcome) = emitted(Script::of([
            Step::Report(FN),
            Step::Fail(io::Error::from_raw_os_error(libc::EACCES)),
        ]));
        assert_eq!(edges, [press(VendorKey::Fn), release(VendorKey::Fn)]);
        assert_eq!(
            outcome.map_err(|error| error.raw_os_error()),
            Err(Some(libc::EACCES))
        );
    }

    #[test]
    fn read_error_while_released_emits_nothing() {
        let (edges, outcome) = emitted(Script::of([Step::Fail(io::Error::from_raw_os_error(
            libc::EIO,
        ))]));
        assert_eq!(edges, []);
        assert!(outcome.is_err());
    }

    #[test]
    fn interrupted_read_keeps_translating() {
        let (edges, outcome) = emitted(Script::of([
            Step::Fail(ErrorKind::Interrupted.into()),
            Step::Report(FN),
            Step::Report(NOTHING),
        ]));
        assert_eq!(edges, [press(VendorKey::Fn), release(VendorKey::Fn)]);
        assert!(outcome.is_ok());
    }

    #[test]
    fn failed_press_stops_translating_and_still_releases() {
        let mut edges: Vec<Edge> = Vec::new();
        let script: Script = Script::of([Step::Report(FN), Step::Report(NOTHING)]);
        let outcome: io::Result<()> = translate(script, |edge: Edge| {
            edges.push(edge);
            match edge.state {
                KeyState::Held => Err(io::Error::from_raw_os_error(libc::EBADF)),
                KeyState::Released => Ok(()),
            }
        });
        assert_eq!(edges, [press(VendorKey::Fn), release(VendorKey::Fn)]);
        assert_eq!(
            outcome.map_err(|error| error.raw_os_error()),
            Err(Some(libc::EBADF))
        );
    }

    #[test]
    fn failed_release_is_reported() {
        let script: Script = Script::of([Step::Report(FN)]);
        let outcome: io::Result<()> = translate(script, |edge: Edge| match edge.state {
            KeyState::Held => Ok(()),
            KeyState::Released => Err(io::Error::from_raw_os_error(libc::EBADF)),
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
