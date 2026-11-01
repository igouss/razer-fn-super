#![no_std]

const REPORT_ID: u8 = 0x04;
const FN_CODE: u8 = 0x01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Released,
    Held,
}

impl Key {
    pub fn after_report(self, report: &[u8]) -> Option<Self> {
        let next: Self = decode(report)?;
        (next != self).then_some(next)
    }

    pub const fn after_end(self) -> Option<Self> {
        match self {
            Self::Held => Some(Self::Released),
            Self::Released => None,
        }
    }
}

fn decode(report: &[u8]) -> Option<Key> {
    match report.split_first() {
        Some((&REPORT_ID, held_codes)) if held_codes.contains(&FN_CODE) => Some(Key::Held),
        Some((&REPORT_ID, _)) => Some(Key::Released),
        Some(_) | None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::Key;

    #[test]
    fn end_of_stream_releases_a_held_key() {
        assert_eq!(Key::Held.after_end(), Some(Key::Released));
    }

    #[test]
    fn end_of_stream_leaves_a_released_key_alone() {
        assert_eq!(Key::Released.after_end(), None);
    }

    #[test]
    fn empty_read_changes_nothing() {
        assert_eq!(Key::Held.after_report(&[]), None);
    }

    #[test]
    fn other_report_ids_change_nothing_even_when_they_carry_the_fn_code() {
        assert_eq!(Key::Released.after_report(&[0x05, 0x01]), None);
    }

    #[test]
    fn other_report_ids_do_not_release_a_held_key() {
        assert_eq!(Key::Held.after_report(&[0x05, 0x00]), None);
    }

    #[test]
    fn report_with_no_held_codes_releases_a_held_key() {
        assert_eq!(Key::Held.after_report(&[0x04]), Some(Key::Released));
    }

    #[test]
    fn report_with_only_other_codes_releases_a_held_key() {
        assert_eq!(
            Key::Held.after_report(&[0x04, 0x02, 0x03]),
            Some(Key::Released)
        );
    }

    #[test]
    fn released_key_stays_released_without_an_edge() {
        assert_eq!(Key::Released.after_report(&[0x04, 0x00]), None);
    }

    #[test]
    fn fn_code_first_presses_a_released_key() {
        assert_eq!(Key::Released.after_report(&[0x04, 0x01]), Some(Key::Held));
    }

    #[test]
    fn fn_code_among_many_presses_a_released_key() {
        assert_eq!(
            Key::Released.after_report(&[0x04, 0x07, 0x09, 0x01, 0x00]),
            Some(Key::Held)
        );
    }

    #[test]
    fn repeated_fn_code_presses_once() {
        assert_eq!(
            Key::Released.after_report(&[0x04, 0x01, 0x01]),
            Some(Key::Held)
        );
    }

    #[test]
    fn held_key_stays_held_without_an_edge() {
        assert_eq!(Key::Held.after_report(&[0x04, 0x01]), None);
    }

    #[test]
    fn report_id_byte_is_not_scanned_for_the_fn_code() {
        assert_eq!(Key::Held.after_report(&[0x04, 0x00]), Some(Key::Released));
    }
}
