//! Opaque tokens: a counter hashed with a per-process random key, so that
//! tokens cannot be guessed from each other; a fixed key in tests.

use std::collections::hash_map::{DefaultHasher, RandomState};
use std::hash::{BuildHasher, Hash, Hasher};

use crate::dto::{DeviceId, FileToken, OpId, PlanToken, ProgramToken};

/// Mints tokens.
#[derive(Debug)]
pub(crate) struct TokenMint {
    key: u64,
    counter: u64,
    ops: u32,
}

impl TokenMint {
    /// A mint with a random key, or the given one.
    pub(crate) fn new(seed: Option<u64>) -> Self {
        let key = seed.unwrap_or_else(|| RandomState::new().hash_one(0x6f77_u64));
        TokenMint {
            key,
            counter: 0,
            ops: 0,
        }
    }

    fn next_hex(&mut self, kind: &str) -> String {
        self.counter = self.counter.wrapping_add(1);
        format!("{:016x}", self.hash(&(kind, self.counter)))
    }

    fn hash(&self, value: &impl Hash) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.key.hash(&mut hasher);
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// A new plan token.
    pub(crate) fn plan(&mut self) -> PlanToken {
        PlanToken(format!("p-{}", self.next_hex("plan")))
    }

    /// A new file token.
    pub(crate) fn file(&mut self) -> FileToken {
        FileToken(format!("f-{}", self.next_hex("file")))
    }

    /// A new program token.
    pub(crate) fn program(&mut self) -> ProgramToken {
        ProgramToken(format!("g-{}", self.next_hex("program")))
    }

    /// A new operation id.
    pub(crate) fn op(&mut self) -> OpId {
        self.ops = self.ops.wrapping_add(1);
        OpId(format!("o-{}", self.ops))
    }

    /// The id of the device at `path` (the same path gives the same id for
    /// the lifetime of the process).
    pub(crate) fn device(&self, path: &str) -> DeviceId {
        DeviceId(format!("d-{:016x}", self.hash(&("device", path))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_have_their_prefixes_and_differ() {
        let mut mint = TokenMint::new(Some(1));
        let a = mint.plan();
        let b = mint.plan();
        assert!(a.0.starts_with("p-") && a.0.len() == 18);
        assert_ne!(a, b);
        assert!(mint.file().0.starts_with("f-"));
        assert_eq!(mint.op().0, "o-1");
        assert_eq!(mint.device("x"), mint.device("x"));
        assert_ne!(mint.device("x"), mint.device("y"));
    }

    #[test]
    fn a_fixed_seed_is_deterministic() {
        let mut a = TokenMint::new(Some(7));
        let mut b = TokenMint::new(Some(7));
        assert_eq!(a.plan(), b.plan());
        let mut c = TokenMint::new(Some(8));
        assert_ne!(TokenMint::new(Some(7)).plan(), c.plan());
    }
}
