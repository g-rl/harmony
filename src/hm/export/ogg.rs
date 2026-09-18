use std::io::Write;
use std::path::Path;

use anyhow::Result;

const PRE_SKIP: u16 = 312;

fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut value = (i as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 0x8000_0000 != 0 {
                (value << 1) ^ 0x04c1_1db7
            } else {
                value << 1
            };
            bit += 1;
        }
        table[i] = value;
        i += 1;
    }
    table
}

fn crc32(data: &[u8]) -> u32 {
    let table = crc_table();
    let mut value = 0u32;
    for byte in data {
        value = (value << 8) ^ table[(((value >> 24) as u8) ^ *byte) as usize];
    }
    value
}

struct Writer {
    serial: u32,
    sequence: u32,
    out: Vec<u8>,
}

impl Writer {
    fn page(&mut self, kind: u8, granule: u64, packets: &[&[u8]]) {
        let mut lacing: Vec<u8> = Vec::new();
        for packet in packets {
            let mut left = packet.len();
            while left >= 255 {
                lacing.push(255);
                left -= 255;
            }
            lacing.push(left as u8);
        }
        let mut page: Vec<u8> = Vec::with_capacity(27 + lacing.len() + 4096);
        page.extend_from_slice(b"OggS");
        page.push(0);
        page.push(kind);
        page.extend_from_slice(&granule.to_le_bytes());
        page.extend_from_slice(&self.serial.to_le_bytes());
        page.extend_from_slice(&self.sequence.to_le_bytes());
        page.extend_from_slice(&[0, 0, 0, 0]);
        page.push(lacing.len() as u8);
        page.extend_from_slice(&lacing);
        for packet in packets {
            page.extend_from_slice(packet);
        }
        let sum = crc32(&page);
        page[22..26].copy_from_slice(&sum.to_le_bytes());
        self.out.extend_from_slice(&page);
        self.sequence += 1;
    }
}

pub fn write(path: &Path, packets: &[&[u8]], channels: u8, samples_per_packet: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut head = Vec::with_capacity(19);
    head.extend_from_slice(b"OpusHead");
    head.push(1);
    head.push(channels.max(1));
    head.extend_from_slice(&PRE_SKIP.to_le_bytes());
    head.extend_from_slice(&48_000u32.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);

    let vendor = b"harmony";
    let mut tags = Vec::new();
    tags.extend_from_slice(b"OpusTags");
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes());

    let mut writer = Writer {
        serial: 0x6861_726d,
        sequence: 0,
        out: Vec::new(),
    };
    writer.page(2, 0, &[&head]);
    writer.page(0, 0, &[&tags]);

    let mut granule = PRE_SKIP as u64;
    let mut batch: Vec<&[u8]> = Vec::new();
    let mut batch_bytes = 0usize;
    for (i, packet) in packets.iter().enumerate() {
        batch.push(packet);
        batch_bytes += packet.len();
        granule += samples_per_packet as u64;
        let last = i + 1 == packets.len();
        if batch.len() == 50 || batch_bytes > 40_000 || last {
            writer.page(if last { 4 } else { 0 }, granule, &batch);
            batch.clear();
            batch_bytes = 0;
        }
    }
    if writer.sequence == 2 {
        writer.page(4, granule, &[&[][..]]);
    }

    let mut file = std::fs::File::create(path)?;
    file.write_all(&writer.out)?;
    Ok(())
}
