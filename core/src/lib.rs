#![no_std]

mod mode;

pub use mode::{Command, FEATURE_REPORT_LEN, Refusal};

const REPORT_ID: u8 = 0x04;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VendorKey {
    Fn,
    Previous,
    PlayPause,
    Next,
    Mute,
}

impl VendorKey {
    pub const ALL: [Self; 5] = [
        Self::Fn,
        Self::Previous,
        Self::PlayPause,
        Self::Next,
        Self::Mute,
    ];

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            0x01 => Some(Self::Fn),
            0x54 => Some(Self::Previous),
            0x55 => Some(Self::PlayPause),
            0x53 => Some(Self::Next),
            0x52 => Some(Self::Mute),
            _ => None,
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::Fn => 1,
            Self::Previous => 1 << 1,
            Self::PlayPause => 1 << 2,
            Self::Next => 1 << 3,
            Self::Mute => 1 << 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyState {
    Released,
    Held,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub key: VendorKey,
    pub state: KeyState,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Held(u8);

impl Held {
    pub const NONE: Self = Self(0);

    pub fn from_report(report: &[u8]) -> Option<Self> {
        match report.split_first() {
            Some((&REPORT_ID, codes)) => Some(
                codes
                    .iter()
                    .filter_map(|&code: &u8| VendorKey::from_code(code))
                    .fold(Self::NONE, Self::with),
            ),
            Some(_) | None => None,
        }
    }

    const fn with(self, key: VendorKey) -> Self {
        Self(self.0 | key.bit())
    }

    pub const fn contains(self, key: VendorKey) -> bool {
        self.0 & key.bit() != 0
    }

    pub fn edges_to(self, next: Self) -> impl Iterator<Item = Edge> {
        VendorKey::ALL
            .into_iter()
            .filter_map(
                move |key: VendorKey| match (self.contains(key), next.contains(key)) {
                    (false, true) => Some(Edge {
                        key,
                        state: KeyState::Held,
                    }),
                    (true, false) => Some(Edge {
                        key,
                        state: KeyState::Released,
                    }),
                    (false, false) | (true, true) => None,
                },
            )
    }

    pub fn after_end(self) -> impl Iterator<Item = Edge> {
        self.edges_to(Self::NONE)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec::Vec;

    use super::{Edge, Held, KeyState, VendorKey};

    fn held(report: &[u8]) -> Held {
        Held::from_report(report).expect("a report 04")
    }

    fn edges(from: Held, to: Held) -> Vec<Edge> {
        from.edges_to(to).collect()
    }

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

    #[test]
    fn empty_read_is_not_a_report() {
        assert_eq!(Held::from_report(&[]), None);
    }

    #[test]
    fn other_report_ids_are_not_reports_even_when_they_carry_the_fn_code() {
        assert_eq!(Held::from_report(&[0x05, 0x01]), None);
    }

    #[test]
    fn report_with_no_codes_holds_nothing() {
        assert_eq!(held(&[0x04]), Held::NONE);
    }

    #[test]
    fn report_with_only_zeros_holds_nothing() {
        assert_eq!(held(&[0x04, 0x00, 0x00]), Held::NONE);
    }

    #[test]
    fn report_id_byte_is_not_scanned_for_codes() {
        assert_eq!(held(&[0x04, 0x00]), Held::NONE);
    }

    #[test]
    fn fn_code_holds_fn() {
        assert!(held(&[0x04, 0x01]).contains(VendorKey::Fn));
    }

    #[test]
    fn previous_code_holds_previous() {
        assert!(held(&[0x04, 0x54]).contains(VendorKey::Previous));
    }

    #[test]
    fn play_pause_code_holds_play_pause() {
        assert!(held(&[0x04, 0x55]).contains(VendorKey::PlayPause));
    }

    #[test]
    fn next_code_holds_next() {
        assert!(held(&[0x04, 0x53]).contains(VendorKey::Next));
    }

    #[test]
    fn mute_code_holds_mute() {
        assert!(held(&[0x04, 0x52]).contains(VendorKey::Mute));
    }

    #[test]
    fn one_code_holds_only_its_key() {
        assert_eq!(
            edges(Held::NONE, held(&[0x04, 0x54])),
            [press(VendorKey::Previous)]
        );
    }

    #[test]
    fn wheel_codes_hold_nothing() {
        assert_eq!(held(&[0x04, 0x50, 0x51]), Held::NONE);
    }

    #[test]
    fn unknown_codes_hold_nothing() {
        assert_eq!(held(&[0x04, 0x02, 0x03, 0x20]), Held::NONE);
    }

    #[test]
    fn code_in_any_slot_is_held() {
        assert!(held(&[0x04, 0x07, 0x09, 0x01, 0x00]).contains(VendorKey::Fn));
    }

    #[test]
    fn repeated_code_holds_its_key_once() {
        assert_eq!(
            edges(Held::NONE, held(&[0x04, 0x01, 0x01])),
            [press(VendorKey::Fn)]
        );
    }

    #[test]
    fn many_codes_hold_every_key() {
        assert_eq!(
            edges(Held::NONE, held(&[0x04, 0x52, 0x53, 0x55, 0x54, 0x01])),
            [
                press(VendorKey::Fn),
                press(VendorKey::Previous),
                press(VendorKey::PlayPause),
                press(VendorKey::Next),
                press(VendorKey::Mute),
            ]
        );
    }

    #[test]
    fn no_change_has_no_edges() {
        assert_eq!(edges(held(&[0x04, 0x01]), held(&[0x04, 0x01])), []);
    }

    #[test]
    fn nothing_to_nothing_has_no_edges() {
        assert_eq!(edges(Held::NONE, Held::NONE), []);
    }

    #[test]
    fn releasing_one_key_leaves_the_other_held() {
        assert_eq!(
            edges(held(&[0x04, 0x01, 0x55]), held(&[0x04, 0x01])),
            [release(VendorKey::PlayPause)]
        );
    }

    #[test]
    fn swapping_keys_releases_one_and_presses_the_other() {
        assert_eq!(
            edges(held(&[0x04, 0x54]), held(&[0x04, 0x53])),
            [release(VendorKey::Previous), press(VendorKey::Next)]
        );
    }

    #[test]
    fn end_with_nothing_held_has_no_edges() {
        assert_eq!(Held::NONE.after_end().collect::<Vec<Edge>>(), []);
    }

    #[test]
    fn end_with_one_held_releases_it() {
        assert_eq!(
            held(&[0x04, 0x01]).after_end().collect::<Vec<Edge>>(),
            [release(VendorKey::Fn)]
        );
    }

    #[test]
    fn end_with_many_held_releases_every_one() {
        assert_eq!(
            held(&[0x04, 0x01, 0x53]).after_end().collect::<Vec<Edge>>(),
            [release(VendorKey::Fn), release(VendorKey::Next)]
        );
    }
}
