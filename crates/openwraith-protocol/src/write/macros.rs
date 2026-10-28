//! Macro programs (spec section 4.9): the keyboard-step macros the vendor
//! site writes, as a validated [`MacroProgram`] that encodes into a macro
//! slot ([`MacroData`]).
//!
//! The encoding is CONFIRMED for a "play once" macro of keyboard steps (c13
//! slot 0, typing test) and for the other three play modes (hardware test
//! H19, 2026-10-27). Semantics follow the site's writer
//! (`agreement.js:1468-1558`): byte 0 = macro ID, byte 1 = play mode, byte 3
//! = step bytes, bytes 4-5 = repeat count (BE16), then 4 bytes per step
//! `[usage, (press << 4) | 0, delay after the step, BE16]`. Mouse and
//! multimedia steps cannot be built: the site's writer and reader disagree
//! about their codes (open question Q13).

use std::fmt;

use super::data::{CanonicalMacro, MACRO_SLOT_LEN, MacroData};
use super::evidence::Evidence;
use super::keymap::HidUsage;
use crate::error::WriteBuildError;
use crate::request::MacroSlotId;

/// Offset of the first step in a macro slot.
const STEPS_OFFSET: usize = 8;

/// Whether a macro step presses or releases its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum KeyMotion {
    /// Key down (step byte 1 = `10`).
    Press,
    /// Key up (step byte 1 = `00`).
    Release,
}

/// One keyboard step of a macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MacroStep {
    /// The key.
    pub usage: HidUsage,
    /// Press or release.
    pub motion: KeyMotion,
    /// Delay to wait after this step, in ms (0-65535, the site's input
    /// range, `wmIndex.js:5320-5322`). The last step's delay must be 0.
    pub delay_after_ms: u16,
}

impl MacroStep {
    /// A press of `usage` followed by `delay_after_ms`.
    pub const fn press(usage: HidUsage, delay_after_ms: u16) -> Self {
        MacroStep {
            usage,
            motion: KeyMotion::Press,
            delay_after_ms,
        }
    }

    /// A release of `usage` followed by `delay_after_ms`.
    pub const fn release(usage: HidUsage, delay_after_ms: u16) -> Self {
        MacroStep {
            usage,
            motion: KeyMotion::Release,
            delay_after_ms,
        }
    }

    /// The 4 bytes of the step in a macro slot.
    const fn bytes(&self) -> [u8; 4] {
        let flags = match self.motion {
            KeyMotion::Press => 0x10,
            KeyMotion::Release => 0x00,
        };
        let [hi, lo] = self.delay_after_ms.to_be_bytes();
        [self.usage.get(), flags, hi, lo]
    }
}

impl fmt::Display for MacroStep {
    /// `A+ 100` (press, then 100 ms) or `B-` (release, no delay).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let motion = match self.motion {
            KeyMotion::Press => '+',
            KeyMotion::Release => '-',
        };
        write!(f, "{}{motion}", self.usage)?;
        if self.delay_after_ms != 0 {
            write!(f, " {}", self.delay_after_ms)?;
        }
        Ok(())
    }
}

/// How a macro plays (slot byte 1, spec section 4.9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum MacroPlay {
    /// 0: once per press (CONFIRMED, c13).
    Once,
    /// 1: `n` times per press, 1..=255 (the site's input range,
    /// `wmIndex.js:5305-5307`; CONFIRMED with 3, H19).
    Times(u8),
    /// 2: one press starts, the next stops (CONFIRMED, H19).
    Toggle,
    /// 3: repeats while held (CONFIRMED, H19).
    WhileHeld,
}

impl MacroPlay {
    /// Slot byte 1.
    pub const fn code(self) -> u8 {
        match self {
            MacroPlay::Once => 0,
            MacroPlay::Times(_) => 1,
            MacroPlay::Toggle => 2,
            MacroPlay::WhileHeld => 3,
        }
    }

    /// Slot bytes 4-5: the repeat count, 1 for every mode but `Times(n)`
    /// (the site writes its "times" input, default 1, whatever the mode).
    pub const fn count(self) -> u16 {
        match self {
            MacroPlay::Times(n) => n as u16,
            MacroPlay::Once | MacroPlay::Toggle | MacroPlay::WhileHeld => 1,
        }
    }
}

/// A validated macro of keyboard steps (spec section 4.9.3): 1-40 steps,
/// every key released by the end, played once, `n` times, as a toggle or
/// while held. [`encode`](Self::encode) gives the slot content.
///
/// ```
/// use openwraith_protocol::write::{HidUsage, MacroPlay, MacroProgram, MacroStep};
/// use openwraith_protocol::MacroSlotId;
///
/// let a = HidUsage::new(0x04).unwrap();
/// let program = MacroProgram::new(
///     MacroPlay::Once,
///     vec![MacroStep::press(a, 30), MacroStep::release(a, 0)],
/// )?;
/// let data = program.encode(MacroSlotId::new(0).unwrap());
/// assert_eq!(data.to_bytes()[..16], [0, 0, 0, 8, 0, 1, 0, 0, 4, 0x10, 0, 30, 4, 0, 0, 0]);
/// # Ok::<(), openwraith_protocol::WriteBuildError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MacroProgram {
    play: MacroPlay,
    steps: Vec<MacroStep>,
}

impl MacroProgram {
    /// Largest number of steps: the site's UI allows 20 press/release
    /// groups (`wmIndex.js:2994-2999`).
    pub const MAX_STEPS: usize = 40;

    /// Checks the program: 1..=40 steps; no key pressed while it is held;
    /// no release without a press; every pressed key released by the end;
    /// no delay after the last step; `Times(0)` refused.
    pub fn new(play: MacroPlay, steps: Vec<MacroStep>) -> Result<Self, WriteBuildError> {
        if steps.len() > Self::MAX_STEPS {
            return Err(WriteBuildError::MacroTooLong {
                steps: steps.len(),
                max: Self::MAX_STEPS,
            });
        }
        if play == MacroPlay::Times(0) {
            return Err(WriteBuildError::InvalidMacroCount);
        }
        let unbalanced = |step, reason| WriteBuildError::UnbalancedMacro { step, reason };
        let Some(last) = steps.last() else {
            return Err(unbalanced(0, "the macro has no steps"));
        };
        // Keys held so far, with the step that pressed them.
        let mut held: Vec<(HidUsage, usize)> = Vec::new();
        for (index, step) in steps.iter().enumerate() {
            let at = held.iter().position(|(usage, _)| *usage == step.usage);
            match (step.motion, at) {
                (KeyMotion::Press, None) => held.push((step.usage, index)),
                (KeyMotion::Press, Some(_)) => {
                    return Err(unbalanced(index, "the key is pressed again while held"));
                }
                (KeyMotion::Release, Some(at)) => {
                    held.remove(at);
                }
                (KeyMotion::Release, None) => {
                    return Err(unbalanced(index, "the key is released without a press"));
                }
            }
        }
        if let Some((_, index)) = held.first() {
            return Err(unbalanced(*index, "the key is still held at the end"));
        }
        if last.delay_after_ms != 0 {
            return Err(WriteBuildError::MacroLastDelay);
        }
        Ok(MacroProgram { play, steps })
    }

    /// How the macro plays.
    pub fn play(&self) -> MacroPlay {
        self.play
    }

    /// The steps.
    pub fn steps(&self) -> &[MacroStep] {
        &self.steps
    }

    /// The slot content: byte 0 = slot number (bit 7 clear), byte 1 = play
    /// mode, byte 3 = 4 x steps, bytes 4-5 = [count](MacroPlay::count)
    /// (BE16), then the steps; every other byte 0.
    pub fn encode(&self, slot: MacroSlotId) -> MacroData {
        let mut bytes = Box::new([0u8; MACRO_SLOT_LEN]);
        bytes[0] = slot.get();
        bytes[1] = self.play.code();
        // At most 40 steps, so at most 160 step bytes.
        bytes[3] = u8::try_from(self.steps.len() * 4).unwrap_or(u8::MAX);
        let [hi, lo] = self.play.count().to_be_bytes();
        bytes[4] = hi;
        bytes[5] = lo;
        for (k, step) in self.steps.iter().enumerate() {
            let at = STEPS_OFFSET + 4 * k;
            if let Some(dst) = bytes.get_mut(at..at + 4) {
                dst.copy_from_slice(&step.bytes());
            }
        }
        MacroData::Content(CanonicalMacro::encoded(bytes))
    }

    /// The program held in a canonical slot, if it is one this type can
    /// express: no copy flag, play mode 0-3 with the count
    /// [`encode`](Self::encode) writes, keyboard steps of offered usages
    /// that satisfy [`new`](Self::new). The macro ID (byte 0, bits 0-6) is
    /// not part of the program.
    pub fn decode(content: &CanonicalMacro) -> Option<MacroProgram> {
        let b = content.as_bytes();
        if b[0] & 0x80 != 0 {
            return None;
        }
        let play = match (b[1], u16::from_be_bytes([b[4], b[5]])) {
            (0, 1) => MacroPlay::Once,
            (1, n) => MacroPlay::Times(u8::try_from(n).ok().filter(|n| *n > 0)?),
            (2, 1) => MacroPlay::Toggle,
            (3, 1) => MacroPlay::WhileHeld,
            _ => return None,
        };
        let steps_end = STEPS_OFFSET + usize::from(content.step_bytes());
        let steps = b
            .get(STEPS_OFFSET..steps_end)?
            .chunks_exact(4)
            .map(|step| {
                let usage = HidUsage::new(*step.first()?)?;
                let motion = match step.get(1)? {
                    0x10 => KeyMotion::Press,
                    0x00 => KeyMotion::Release,
                    _ => return None,
                };
                let delay_after_ms = u16::from_be_bytes([*step.get(2)?, *step.get(3)?]);
                Some(MacroStep {
                    usage,
                    motion,
                    delay_after_ms,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        MacroProgram::new(play, steps).ok()
    }

    /// Every play mode is CONFIRMED: "play once" by c13, the others by
    /// hardware test H19 (2026-10-27, Q13: three times typed "aaa", toggle
    /// starts and stops, while-held repeats while held).
    pub fn evidence(&self) -> Evidence {
        Evidence::Confirmed
    }
}

impl fmt::Display for MacroProgram {
    /// The steps as `A+ 100, A- 100, B+ 100, B-`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, step) in self.steps.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{step}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::test_util::table;
    use crate::request::ReadRequest;

    fn usage(u: u8) -> HidUsage {
        HidUsage::new(u).unwrap()
    }

    /// c13 slot 0: A down 100 ms, A up 100 ms, B down 100 ms, B up.
    fn c13_program() -> MacroProgram {
        let (a, b) = (usage(0x04), usage(0x05));
        MacroProgram::new(
            MacroPlay::Once,
            vec![
                MacroStep::press(a, 100),
                MacroStep::release(a, 100),
                MacroStep::press(b, 100),
                MacroStep::release(b, 0),
            ],
        )
        .unwrap()
    }

    #[test]
    fn encodes_the_c13_slot() {
        let data = c13_program().encode(MacroSlotId::new(0).unwrap());
        let bytes = data.to_bytes();
        assert_eq!(
            bytes[..24],
            [
                0x00, 0x00, 0x00, 0x10, 0x00, 0x01, 0x00, 0x00, 0x04, 0x10, 0x00, 0x64, 0x04, 0x00,
                0x00, 0x64, 0x05, 0x10, 0x00, 0x64, 0x05, 0x00, 0x00, 0x00
            ]
        );
        assert!(bytes[24..].iter().all(|b| *b == 0));
        assert_eq!(c13_program().to_string(), "A+ 100, A- 100, B+ 100, B-");
        assert!(c13_program().evidence().is_confirmed());
    }

    #[test]
    fn play_modes_and_slot_byte() {
        let a = usage(0x04);
        let steps = vec![MacroStep::press(a, 0), MacroStep::release(a, 0)];
        for (play, code, count) in [
            (MacroPlay::Once, 0, 1),
            (MacroPlay::Times(3), 1, 3),
            (MacroPlay::Times(255), 1, 255),
            (MacroPlay::Toggle, 2, 1),
            (MacroPlay::WhileHeld, 3, 1),
        ] {
            let program = MacroProgram::new(play, steps.clone()).unwrap();
            let bytes = program.encode(MacroSlotId::new(9).unwrap()).to_bytes();
            assert_eq!(bytes[0], 9);
            assert_eq!(bytes[1], code);
            assert_eq!(u16::from_be_bytes([bytes[4], bytes[5]]), count);
            // H19 tested every play mode.
            assert!(program.evidence().is_confirmed(), "{play:?}");
        }
        assert_eq!(
            MacroProgram::new(MacroPlay::Times(0), steps),
            Err(WriteBuildError::InvalidMacroCount)
        );
    }

    #[test]
    fn rejects_unbalanced_programs() {
        let (a, b) = (usage(0x04), usage(0x05));
        let cases: [(Vec<MacroStep>, WriteBuildError); 5] = [
            (
                vec![],
                WriteBuildError::UnbalancedMacro {
                    step: 0,
                    reason: "the macro has no steps",
                },
            ),
            (
                vec![MacroStep::release(a, 0)],
                WriteBuildError::UnbalancedMacro {
                    step: 0,
                    reason: "the key is released without a press",
                },
            ),
            (
                vec![
                    MacroStep::press(a, 0),
                    MacroStep::press(a, 0),
                    MacroStep::release(a, 0),
                ],
                WriteBuildError::UnbalancedMacro {
                    step: 1,
                    reason: "the key is pressed again while held",
                },
            ),
            (
                vec![
                    MacroStep::press(a, 0),
                    MacroStep::press(b, 0),
                    MacroStep::release(a, 0),
                ],
                WriteBuildError::UnbalancedMacro {
                    step: 1,
                    reason: "the key is still held at the end",
                },
            ),
            (
                vec![MacroStep::press(a, 0), MacroStep::release(a, 5)],
                WriteBuildError::MacroLastDelay,
            ),
        ];
        for (steps, error) in cases {
            assert_eq!(MacroProgram::new(MacroPlay::Once, steps), Err(error));
        }
        // 20 press/release pairs are allowed, 21 are not.
        let pairs = |n: usize| -> Vec<MacroStep> {
            (0..n)
                .flat_map(|_| [MacroStep::press(a, 1), MacroStep::release(a, 0)])
                .collect()
        };
        assert!(MacroProgram::new(MacroPlay::Once, pairs(20)).is_ok());
        assert_eq!(
            MacroProgram::new(MacroPlay::Once, pairs(21)),
            Err(WriteBuildError::MacroTooLong { steps: 42, max: 40 })
        );
        // Overlapping keys are fine (A down, B down, A up, B up).
        assert!(
            MacroProgram::new(
                MacroPlay::Once,
                vec![
                    MacroStep::press(a, 10),
                    MacroStep::press(b, 10),
                    MacroStep::release(a, 10),
                    MacroStep::release(b, 0),
                ]
            )
            .is_ok()
        );
    }

    /// The canonical slot of `bytes` as read back.
    fn read_back(slot: MacroSlotId, data: &MacroData) -> MacroData {
        let raw = table(ReadRequest::Macro(slot), &data.to_bytes());
        MacroData::from_response(&raw).unwrap().1
    }

    #[test]
    fn encode_is_canonical_and_decodes_back() {
        // Every play mode with up to 40 steps of random usages and delays.
        let mut seed = 0x1234_5678_9ABC_DEF0u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let usages: Vec<HidUsage> = HidUsage::all().collect();
        for round in 0..300 {
            let pairs = 1 + usize::try_from(next() % 20).unwrap();
            let mut steps = Vec::new();
            for k in 0..pairs {
                let u = usages[usize::try_from(next() % 122).unwrap()];
                // Keep keys distinct within a pair; nest every second pair.
                let delay = u16::try_from(next() % 65536).unwrap();
                steps.push(MacroStep::press(u, delay));
                steps.push(MacroStep::release(
                    u,
                    if k + 1 == pairs { 0 } else { delay },
                ));
            }
            let play = match round % 5 {
                0 => MacroPlay::Once,
                1 => MacroPlay::Times(1 + u8::try_from(next() % 255).unwrap()),
                2 => MacroPlay::Toggle,
                3 => MacroPlay::WhileHeld,
                _ => MacroPlay::Times(1),
            };
            let program = MacroProgram::new(play, steps).unwrap();
            let slot = MacroSlotId::new(u8::try_from(round % 10).unwrap()).unwrap();
            let data = program.encode(slot);
            assert_eq!(read_back(slot, &data), data, "round {round}");
            let MacroData::Content(content) = &data else {
                panic!("an encoded macro is never empty");
            };
            assert_eq!(MacroProgram::decode(content).as_ref(), Some(&program));
        }
    }

    #[test]
    fn decode_refuses_what_it_cannot_express() {
        let slot = MacroSlotId::new(0).unwrap();
        let base = c13_program().encode(slot).to_bytes();
        let decode = |bytes: &[u8]| {
            let raw = table(ReadRequest::Macro(slot), bytes);
            match MacroData::from_response(&raw).unwrap().1 {
                MacroData::Content(content) => MacroProgram::decode(&content),
                MacroData::Empty => None,
            }
        };
        assert_eq!(decode(&base), Some(c13_program()));
        // The macro ID is not part of the program.
        let mut id = base;
        id[0] = 0x05;
        assert_eq!(decode(&id), Some(c13_program()));
        for (offset, value) in [
            (0, 0x80),  // copy flag
            (5, 0x02),  // "once" with count 2
            (9, 0x11),  // mouse step
            (8, 0x32),  // usage not offered
            (21, 0x10), // B pressed twice, never released
        ] {
            let mut bytes = base;
            bytes[offset] = value;
            assert_eq!(decode(&bytes), None, "offset {offset}");
        }
        // Times with count 0 or above 255.
        let mut times = base;
        times[1] = 1;
        times[5] = 0;
        assert_eq!(decode(&times), None);
        times[4] = 1;
        assert_eq!(decode(&times), None);
        times[4] = 0;
        times[5] = 7;
        assert_eq!(decode(&times).map(|p| p.play()), Some(MacroPlay::Times(7)));
    }
}
