use std::fs;
use std::process;

use crate::{slot_checksum_start, slot_data_range, HEADER_SIZE, NUM_SLOTS, SLOT_STRIDE};
use crate::save::{find_character, read_u32_le, write_u32_le};

pub struct Edits {
    pub vigor: u32,
    pub mind: u32,
    pub endurance: u32,
    pub strength: u32,
    pub dexterity: u32,
    pub intelligence: u32,
    pub faith: u32,
    pub arcane: u32,
    pub level: u32,
    pub runes: u32,
}

pub struct Current {
    pub stats: [u32; 8],
    pub level: u32,
    pub runes: u32,
    pub character_name: &'static str,
}

pub fn run(input: &str, output: &str) {
    let current = Current {
        stats: [12, 11, 13, 23, 16, 10, 8, 8],
        level: 22,
        runes: 2352,
        character_name: "Jose",
    };

    let edits = Edits {
        vigor: 12,
        mind: 11,
        endurance: 13,
        strength: 23,
        dexterity: 16,
        intelligence: 10,
        faith: 8,
        arcane: 8,
        level: 22,
        runes: 5000,
    };

    let stats_sum: u32 = edits.vigor + edits.mind + edits.endurance + edits.strength
        + edits.dexterity + edits.intelligence + edits.faith + edits.arcane;
    let expected_level = stats_sum.saturating_sub(79);
    if expected_level != edits.level {
        eprintln!("level/stats inconsistency: sum={} expects level {}, got {}",
            stats_sum, expected_level, edits.level);
        process::exit(1);
    }

    let mut bytes = match fs::read(input) {
        Ok(b) => b,
        Err(e) => { eprintln!("read {}: {}", input, e); process::exit(1); }
    };

    println!("Read {} bytes from {}", bytes.len(), input);

    if &bytes[0..4] != b"BND4" {
        eprintln!("not a BND4 save file (magic was {:?})", &bytes[0..4]);
        process::exit(1);
    }

    let (slot_idx, pgd_start) = match find_character(&bytes, &current.stats, current.level, current.runes) {
        Some(v) => v,
        None => {
            eprintln!("could not find stat signature matching expected values");
            eprintln!("expected: stats={:?} level={} runes={}", current.stats, current.level, current.runes);
            eprintln!("has the character changed since the screenshot?");
            process::exit(1);
        }
    };

    let stat_abs_offset = pgd_start + 52;
    println!("Found character in slot {} (stats at file offset 0x{:x})", slot_idx, stat_abs_offset);

    let level_abs = stat_abs_offset + 44;
    let runes_abs = stat_abs_offset + 48;
    let runes_memory_abs = stat_abs_offset + 52;

    let current_runes_memory = read_u32_le(&bytes, runes_memory_abs);
    println!("Current runes memory: {}", current_runes_memory);

    let new_stats = [edits.vigor, edits.mind, edits.endurance, edits.strength,
                     edits.dexterity, edits.intelligence, edits.faith, edits.arcane];
    for (i, s) in new_stats.iter().enumerate() {
        write_u32_le(&mut bytes, stat_abs_offset + i * 4, *s);
    }
    write_u32_le(&mut bytes, level_abs, edits.level);
    write_u32_le(&mut bytes, runes_abs, edits.runes);

    if edits.runes > current.runes {
        let extra = edits.runes - current.runes;
        let new_memory = current_runes_memory.saturating_add(extra);
        write_u32_le(&mut bytes, runes_memory_abs, new_memory);
    }

    let (slot_data_start, slot_data_end) = slot_data_range(slot_idx);
    let checksum_start = slot_checksum_start(slot_idx);
    let digest = md5::compute(&bytes[slot_data_start..slot_data_end]);
    bytes[checksum_start..checksum_start+16].copy_from_slice(&digest.0);
    println!("Recomputed slot {} checksum: {:x?}", slot_idx, digest.0);

    let user_data_10_start = HEADER_SIZE + NUM_SLOTS * SLOT_STRIDE;
    if user_data_10_start < bytes.len() {
        let name_u16: Vec<u16> = current.character_name.encode_utf16().collect();
        let mut name_bytes: Vec<u8> = Vec::with_capacity(name_u16.len() * 2 + 2);
        for c in &name_u16 { name_bytes.extend(&c.to_le_bytes()); }
        name_bytes.extend(&[0u8, 0u8]);

        if let Some(rel) = bytes[user_data_10_start..].windows(name_bytes.len())
            .position(|w| w == name_bytes.as_slice())
        {
            let name_abs = user_data_10_start + rel;
            let ps_level_abs = name_abs + 34;
            if ps_level_abs + 4 <= bytes.len() {
                let ps_level = read_u32_le(&bytes, ps_level_abs);
                if ps_level == current.level {
                    write_u32_le(&mut bytes, ps_level_abs, edits.level);
                    println!("Updated profile_summary level at file offset 0x{:x}", ps_level_abs);
                } else {
                    println!("profile_summary level was {} (expected {}) — skipped", ps_level, current.level);
                }
            }
        } else {
            println!("profile_summary name not found — character selection menu may show old level until first in-game save");
        }
    }

    if let Err(e) = fs::write(output, &bytes) {
        eprintln!("write {}: {}", output, e);
        process::exit(1);
    }

    println!();
    println!("=== Stealth edit applied ===");
    println!("  Level:     {} -> {}", current.level, edits.level);
    println!("  Vigor:     {} -> {}", current.stats[0], edits.vigor);
    println!("  Mind:      {} -> {}", current.stats[1], edits.mind);
    println!("  Endurance: {} -> {}", current.stats[2], edits.endurance);
    println!("  Strength:  {} -> {}", current.stats[3], edits.strength);
    println!("  Dexterity: {} -> {}", current.stats[4], edits.dexterity);
    println!("  Runes:     {} -> {}", current.runes, edits.runes);
    println!();
    println!("Wrote {} bytes to {}", bytes.len(), output);
}
