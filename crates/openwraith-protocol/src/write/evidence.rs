//! How well the effect of a written value is backed by evidence (spec
//! section 0.1).
//!
//! Every write builder produces only byte forms that the vendor site sends
//! and the keyboard accepts. For some of them it is not yet verified on a
//! W75 that the keyboard does what the site's label says. Plans show these
//! as "not yet verified"; an [`UnverifiedNote`] names what is unverified
//! and the open question of spec section 7.1 that tracks it. The notes are
//! flipped to [`Evidence::Confirmed`] one by one as hardware tests pass.

use std::fmt;

/// Whether the effect of a write value is verified on a W75.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Evidence {
    /// Captured from the vendor site and tested on a W75 (CONFIRMED).
    Confirmed,
    /// The vendor site sends the same bytes, but what the keyboard does with
    /// them is not verified.
    Unverified(UnverifiedNote),
}

impl Evidence {
    /// An [`Evidence::Unverified`] with `what` and `question`.
    pub const fn unverified(what: &'static str, question: &'static str) -> Evidence {
        Evidence::Unverified(UnverifiedNote::new(what, question))
    }

    /// Whether this is [`Evidence::Confirmed`].
    pub const fn is_confirmed(&self) -> bool {
        matches!(self, Evidence::Confirmed)
    }

    /// The note of an unverified value.
    pub const fn note(&self) -> Option<UnverifiedNote> {
        match self {
            Evidence::Confirmed => None,
            Evidence::Unverified(note) => Some(*note),
        }
    }
}

/// What is not verified about a write value, and where it is tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct UnverifiedNote {
    /// Short description, e.g. `"Mod-Tap hold path"`.
    pub what: &'static str,
    /// The open question of spec section 7.1, e.g. `"Q19"`.
    pub question: &'static str,
}

impl UnverifiedNote {
    /// A note about `what`, tracked by open question `question`.
    pub const fn new(what: &'static str, question: &'static str) -> Self {
        UnverifiedNote { what, question }
    }
}

impl fmt::Display for UnverifiedNote {
    /// E.g. `Mod-Tap hold path (Q19)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.what, self.question)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_accessors() {
        assert!(Evidence::Confirmed.is_confirmed());
        assert_eq!(Evidence::Confirmed.note(), None);
        let e = Evidence::unverified("Mod-Tap hold path", "Q19");
        assert!(!e.is_confirmed());
        assert_eq!(
            e.note().map(|n| n.to_string()).as_deref(),
            Some("Mod-Tap hold path (Q19)")
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serializes() {
        assert_eq!(
            serde_json::to_string(&Evidence::unverified("x", "Q1")).unwrap(),
            r#"{"unverified":{"what":"x","question":"Q1"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Evidence::Confirmed).unwrap(),
            r#""confirmed""#
        );
    }
}
