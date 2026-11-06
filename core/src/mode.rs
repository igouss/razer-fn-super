use core::fmt;

pub const FEATURE_REPORT_LEN: usize = 91;

const TRANSACTION_ID: u8 = 0x1F;
const COMMAND_CLASS: u8 = 0x00;
const DATA_SIZE: u8 = 0x02;
const STATUS_SUCCESSFUL: u8 = 0x02;
const DRIVER_MODE: u8 = 0x03;
const STATUS: usize = 1;
const TRANSACTION: usize = 2;
const SIZE: usize = 6;
const CLASS: usize = 7;
const COMMAND: usize = 8;
const ARGUMENTS: usize = 9;
const CHECKSUM: usize = 89;
const CHECKSUMMED: core::ops::Range<usize> = 3..CHECKSUM;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    SetDriverMode,
    GetMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Length(usize),
    Status(u8),
    Echo { class: u8, command: u8 },
    Mode(u8),
}

impl Command {
    pub const HANDSHAKE: [Self; 2] = [Self::SetDriverMode, Self::GetMode];

    const fn id(self) -> u8 {
        match self {
            Self::SetDriverMode => 0x04,
            Self::GetMode => 0x84,
        }
    }

    const fn arguments(self) -> [u8; 2] {
        match self {
            Self::SetDriverMode => [DRIVER_MODE, 0x00],
            Self::GetMode => [0x00, 0x00],
        }
    }

    pub fn request(self) -> [u8; FEATURE_REPORT_LEN] {
        let mut report: [u8; FEATURE_REPORT_LEN] = [0; FEATURE_REPORT_LEN];
        report[TRANSACTION] = TRANSACTION_ID;
        report[SIZE] = DATA_SIZE;
        report[CLASS] = COMMAND_CLASS;
        report[COMMAND] = self.id();
        report[ARGUMENTS..ARGUMENTS + 2].copy_from_slice(&self.arguments());
        report[CHECKSUM] = report[CHECKSUMMED]
            .iter()
            .fold(0, |sum: u8, byte: &u8| sum ^ byte);
        report
    }

    pub fn check(self, response: &[u8]) -> Result<(), Refusal> {
        if response.len() != FEATURE_REPORT_LEN {
            return Err(Refusal::Length(response.len()));
        }
        if response[STATUS] != STATUS_SUCCESSFUL {
            return Err(Refusal::Status(response[STATUS]));
        }
        if (response[CLASS], response[COMMAND]) != (COMMAND_CLASS, self.id()) {
            return Err(Refusal::Echo {
                class: response[CLASS],
                command: response[COMMAND],
            });
        }
        match self {
            Self::SetDriverMode => Ok(()),
            Self::GetMode if response[ARGUMENTS] == DRIVER_MODE => Ok(()),
            Self::GetMode => Err(Refusal::Mode(response[ARGUMENTS])),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Length(length) => write!(
                formatter,
                "response is {length} bytes, not {FEATURE_REPORT_LEN}"
            ),
            Self::Status(status) => write!(formatter, "status {status:#04x}, not successful"),
            Self::Echo { class, command } => write!(
                formatter,
                "response answers command {class:#04x}/{command:#04x}"
            ),
            Self::Mode(mode) => write!(formatter, "device mode is {mode:#04x}, not driver mode"),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::string::ToString;

    use super::{Command, FEATURE_REPORT_LEN, Refusal};

    const SET_REQUEST_HEAD: [u8; 11] = [
        0x00, 0x00, 0x1f, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x03, 0x00,
    ];
    const GET_REQUEST_HEAD: [u8; 11] = [
        0x00, 0x00, 0x1f, 0x00, 0x00, 0x00, 0x02, 0x00, 0x84, 0x00, 0x00,
    ];

    fn response(head: &[u8]) -> [u8; FEATURE_REPORT_LEN] {
        let mut report: [u8; FEATURE_REPORT_LEN] = [0; FEATURE_REPORT_LEN];
        report[..head.len()].copy_from_slice(head);
        report
    }

    const SET_ANSWERED: [u8; 11] = [
        0x00, 0x02, 0x1f, 0x00, 0x00, 0x00, 0x02, 0x00, 0x04, 0x03, 0x00,
    ];
    const DRIVER_MODE_READ: [u8; 11] = [
        0x00, 0x02, 0x1f, 0x00, 0x00, 0x00, 0x01, 0x00, 0x84, 0x03, 0x00,
    ];

    #[test]
    fn set_driver_mode_request_matches_the_bytes_the_keyboard_accepted() {
        let request: [u8; FEATURE_REPORT_LEN] = Command::SetDriverMode.request();
        assert_eq!(request[..11], SET_REQUEST_HEAD);
        assert_eq!(request[89], 0x05);
        assert!(request[11..89].iter().all(|&byte: &u8| byte == 0));
        assert_eq!(request[90], 0);
    }

    #[test]
    fn get_mode_request_matches_the_bytes_the_keyboard_accepted() {
        let request: [u8; FEATURE_REPORT_LEN] = Command::GetMode.request();
        assert_eq!(request[..11], GET_REQUEST_HEAD);
        assert_eq!(request[89], 0x86);
    }

    #[test]
    fn handshake_sets_then_reads_back() {
        assert_eq!(
            Command::HANDSHAKE,
            [Command::SetDriverMode, Command::GetMode]
        );
    }

    #[test]
    fn answered_set_is_accepted() {
        assert_eq!(
            Command::SetDriverMode.check(&response(&SET_ANSWERED)),
            Ok(())
        );
    }

    #[test]
    fn driver_mode_read_back_is_accepted() {
        assert_eq!(Command::GetMode.check(&response(&DRIVER_MODE_READ)), Ok(()));
    }

    #[test]
    fn empty_response_is_refused() {
        assert_eq!(Command::GetMode.check(&[]), Err(Refusal::Length(0)));
    }

    #[test]
    fn short_response_is_refused() {
        assert_eq!(
            Command::GetMode.check(&DRIVER_MODE_READ),
            Err(Refusal::Length(11))
        );
    }

    #[test]
    fn busy_status_is_refused() {
        let mut busy: [u8; FEATURE_REPORT_LEN] = response(&SET_ANSWERED);
        busy[1] = 0x01;
        assert_eq!(
            Command::SetDriverMode.check(&busy),
            Err(Refusal::Status(0x01))
        );
    }

    #[test]
    fn not_supported_status_is_refused() {
        let mut unsupported: [u8; FEATURE_REPORT_LEN] = response(&DRIVER_MODE_READ);
        unsupported[1] = 0x05;
        assert_eq!(
            Command::GetMode.check(&unsupported),
            Err(Refusal::Status(0x05))
        );
    }

    #[test]
    fn answer_to_another_command_is_refused() {
        assert_eq!(
            Command::GetMode.check(&response(&SET_ANSWERED)),
            Err(Refusal::Echo {
                class: 0x00,
                command: 0x04
            })
        );
    }

    #[test]
    fn answer_from_another_command_class_is_refused() {
        let mut other_class: [u8; FEATURE_REPORT_LEN] = response(&DRIVER_MODE_READ);
        other_class[7] = 0x03;
        assert_eq!(
            Command::GetMode.check(&other_class),
            Err(Refusal::Echo {
                class: 0x03,
                command: 0x84
            })
        );
    }

    #[test]
    fn normal_mode_read_back_is_refused() {
        let mut normal: [u8; FEATURE_REPORT_LEN] = response(&DRIVER_MODE_READ);
        normal[9] = 0x00;
        assert_eq!(Command::GetMode.check(&normal), Err(Refusal::Mode(0x00)));
    }

    #[test]
    fn set_response_does_not_need_driver_mode_in_its_arguments() {
        let mut no_echo: [u8; FEATURE_REPORT_LEN] = response(&SET_ANSWERED);
        no_echo[9] = 0x00;
        assert_eq!(Command::SetDriverMode.check(&no_echo), Ok(()));
    }

    #[test]
    fn length_refusal_names_both_lengths() {
        assert_eq!(
            Refusal::Length(11).to_string(),
            "response is 11 bytes, not 91"
        );
    }

    #[test]
    fn status_refusal_names_the_status() {
        assert_eq!(
            Refusal::Status(0x05).to_string(),
            "status 0x05, not successful"
        );
    }

    #[test]
    fn echo_refusal_names_the_answered_command() {
        assert_eq!(
            Refusal::Echo {
                class: 0x00,
                command: 0x04
            }
            .to_string(),
            "response answers command 0x00/0x04"
        );
    }

    #[test]
    fn mode_refusal_names_the_mode() {
        assert_eq!(
            Refusal::Mode(0x00).to_string(),
            "device mode is 0x00, not driver mode"
        );
    }
}
