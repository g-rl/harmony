//! The hashes the engines use in place of names, and the ones the published
//! name databases are keyed by.
//!
//! Harmony ships no names. What it can do is take the hash of a name someone
//! else has already recovered and see whether it matches an entry, which means
//! knowing every function those databases are built with.

pub const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
/// The offset the newer infinity ward line hashes asset names with.
pub const ASSET_OFFSET: u64 = 0x47F5_817A_5EF9_61BA;
/// The offset and prime the script names of modern warfare ii and iii use.
pub const SCRIPT_OFFSET: u64 = 0x79D6_530B_0BB9_B5D1;
pub const SCRIPT_PRIME: u64 = 0x0000_0100_0000_0233;
/// What the packages keep of an asset hash: the low sixty bits.
pub const NAME_MASK: u64 = 0x0FFF_FFFF_FFFF_FFFF;
/// What the published databases keep of one: the low sixty three.
pub const ASSET_MASK: u64 = u64::MAX >> 1;

/// The engine's own fold before hashing: upper case to lower, and a windows
/// separator to a forward one, so `Weapons\\AK47` and `weapons/ak47` are one
/// name.
fn fold(byte: u8) -> u8 {
    match byte {
        b'\\' => b'/',
        other => other.to_ascii_lowercase(),
    }
}

fn fnv(text: &str, offset: u64, prime: u64, folded: bool) -> u64 {
    let mut hash = offset;
    for byte in text.as_bytes() {
        let byte = if folded { fold(*byte) } else { *byte };
        hash ^= byte as u64;
        hash = hash.wrapping_mul(prime);
    }
    hash
}

/// An asset name in modern warfare ii and newer.
pub fn asset(text: &str) -> u64 {
    fnv(text, ASSET_OFFSET, FNV_PRIME, true) & ASSET_MASK
}

/// Plain fnv-1a over the name exactly as written, which is what the treyarch
/// databases are keyed by.
pub fn fnv1a64_exact(text: &str) -> u64 {
    fnv(text, FNV_OFFSET, FNV_PRIME, false)
}

/// A script name in modern warfare ii and iii.
pub fn script(text: &str) -> u64 {
    fnv(text, SCRIPT_OFFSET, SCRIPT_PRIME, true)
}

pub const FNV32_OFFSET: u32 = 0x811C_9DC5;
pub const FNV32_PRIME: u32 = 0x0100_0193;

/// Fnv-1a over thirty two bits, which is what the black ops ii banks key their
/// entries by: their tables hold a `u32`, not a `u64`, so no sixty four bit
/// function can ever match one.
pub fn fnv1a32(text: &str) -> u64 {
    let mut hash = FNV32_OFFSET;
    for byte in text.as_bytes() {
        hash ^= fold(*byte) as u32;
        hash = hash.wrapping_mul(FNV32_PRIME);
    }
    hash as u64
}

/// Fnv-1a over the name folded the way the engine folds it.
pub fn fnv1a64(text: &str) -> u64 {
    fnv(text, FNV_OFFSET, FNV_PRIME, true)
}

pub fn iw_name(text: &str) -> u64 {
    fnv1a64(text) & NAME_MASK
}

pub fn dbj2(text: &str) -> u32 {
    let mut hash: u32 = 5381;
    for byte in text.as_bytes() {
        let c = byte.to_ascii_lowercase() as u32;
        hash = c
            .wrapping_add(hash << 6)
            .wrapping_add(hash << 16)
            .wrapping_sub(hash);
    }
    hash
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HashFn {
    Fnv1a64,
    IwName,
    Dbj2,
    /// Asset names in the newer infinity ward titles.
    Asset,
    /// Fnv-1a with no folding at all.
    Exact,
    /// Script names in modern warfare ii and iii.
    Script,
    /// Thirty two bit fnv-1a, for the older treyarch banks.
    Fnv1a32,
}

impl HashFn {
    pub fn of(self, text: &str) -> u64 {
        match self {
            HashFn::Fnv1a64 => fnv1a64(text),
            HashFn::IwName => iw_name(text),
            HashFn::Dbj2 => dbj2(text) as u64,
            HashFn::Asset => asset(text),
            HashFn::Exact => fnv1a64_exact(text),
            HashFn::Script => script(text),
            HashFn::Fnv1a32 => fnv1a32(text),
        }
    }

    /// Every function a wordlist is worth trying under.
    pub fn all() -> [HashFn; 7] {
        [
            HashFn::Fnv1a64,
            HashFn::IwName,
            HashFn::Dbj2,
            HashFn::Asset,
            HashFn::Exact,
            HashFn::Script,
            HashFn::Fnv1a32,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_separator_hashes_as_a_forward_one() {
        assert_eq!(fnv1a64("weapons\\ak47"), fnv1a64("weapons/ak47"));
        assert_eq!(asset("Weapons\\AK47"), asset("weapons/ak47"));
    }

    #[test]
    fn an_asset_hash_is_sixty_three_bits() {
        assert_eq!(asset("anything at all") >> 63, 0);
    }

    #[test]
    fn the_exact_function_keeps_its_case() {
        assert_ne!(fnv1a64_exact("Name"), fnv1a64_exact("name"));
        assert_eq!(fnv1a64_exact(""), FNV_OFFSET);
    }
}
