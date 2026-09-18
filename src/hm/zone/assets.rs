use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct Inventory {
    pub strings: usize,
    pub assets: usize,
    pub array_at: usize,
    pub array_end: usize,
    pub by_type: BTreeMap<u64, usize>,
}

fn u32at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn u64at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

fn is_record(z: &[u8], o: usize) -> bool {
    if o + 16 > z.len() {
        return false;
    }
    let kind = u64at(z, o);
    let pointer = u64at(z, o + 8);
    kind < 0x200 && (pointer == u64::MAX || pointer == u64::MAX - 1)
}

pub fn inventory(zone: &[u8]) -> Option<Inventory> {
    if zone.len() < 0x40 {
        return None;
    }
    let strings = u32at(zone, 0) as usize;
    let assets = u32at(zone, 0x10) as usize;
    if assets == 0 || assets > 4_000_000 {
        return None;
    }

    let from = 0x20 + strings * 8;
    let mut at = from;
    let mut found = None;
    while at + 16 * 8 < zone.len() {
        if (0..8).all(|i| is_record(zone, at + i * 16)) {
            found = Some(at);
            break;
        }
        at += 1;
    }
    let array_at = found?;

    let mut by_type: BTreeMap<u64, usize> = BTreeMap::new();
    let mut read = 0usize;
    while read < assets && is_record(zone, array_at + read * 16) {
        *by_type.entry(u64at(zone, array_at + read * 16)).or_default() += 1;
        read += 1;
    }

    Some(Inventory {
        strings,
        assets: read,
        array_at,
        array_end: array_at + read * 16,
        by_type,
    })
}
