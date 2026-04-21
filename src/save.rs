use crate::{slot_data_range, NUM_SLOTS};

// PlayerGameData layout (relative offsets from start of PlayerGameData):
//   +0..+52   misc (HP/FP/SP triples)
//   +52..+84  stats (vigor..arcane, 8 u32)
//   +84..+96  unknown padding (3 i32)
//   +96       level (u32)
//   +100      souls (u32)        <- runes in hand
//   +104      soulsmemory (u32)  <- rune-loss target
//   +108..+148  unknown 40-byte block
//   +148..+180  character_name  (UTF-16LE, 16 wide chars max, null-terminated)
//   +180..+182  unknown 2 bytes
//   +182       gender (u8)
//   +183       arche_type / starting class (u8)
//   +187       gift (u8)
//   ...
//   +432       end of PlayerGameData
//
// After PlayerGameData:
//   +432..+640 _0xd0 padding (208 bytes)
//   +640..+728 EquipData  (88 bytes)
//   +728..+844 ChrAsm     (116 bytes)
//   +844..+932 ChrAsm2    (88 bytes)
//   +932       EquipInventoryData starts

pub const STATS_OFFSET_IN_PGD: usize = 52;
pub const PGD_SIZE: usize = 432;
pub const EQUIP_DATA_OFFSET_FROM_PGD: usize = 640;        // after PGD(432) + _0xd0(208)
pub const EQUIP_INVENTORY_OFFSET_FROM_PGD: usize = 932;   // after EquipData(88) + ChrAsm(116) + ChrAsm2(88)

pub const PGD_LEVEL: usize = 96;
pub const PGD_SOULS: usize = 100;
pub const PGD_SOULS_MEMORY: usize = 104;
pub const PGD_CHARACTER_NAME: usize = 148;
pub const PGD_CHARACTER_NAME_LEN_BYTES: usize = 32;
pub const PGD_GENDER: usize = 182;
pub const PGD_ARCHE_TYPE: usize = 183;
pub const PGD_GIFT: usize = 187;

pub const COMMON_INVENTORY_SLOTS: usize = 0xa80; // 2688
pub const KEY_INVENTORY_SLOTS: usize = 0x180;    // 384
pub const INVENTORY_ITEM_SIZE: usize = 12;       // ga_item_handle + quantity + inventory_index (3 u32)

// GaItem table (top of slot, right after a 32-byte preamble):
//   ver(4) + map_id(4) + _0x18(0x18) = 32 bytes, then 0x1400 = 5120 GaItem entries.
// Each GaItem is:
//   +0 gaitem_handle: u32  (always)
//   +4 item_id: u32        (always)
//   Then conditional on (item_id & 0xf0000000):
//     0x00000000 (weapon, but only if item_id != 0): +12 unk2,unk3,aow_handle,unk5 = 13 extra bytes
//     0x10000000 (armor): 8 extra bytes (unk2,unk3)
//     otherwise: 0 extra bytes
//
// Item ID category flags (high nibble):
//   0x00000000  Weapon (or empty slot if id==0)
//   0x10000000  Armor
//   0x20000000  Talisman/Accessory
//   0x40000000  Goods / consumables / crafting materials
//   0x80000000  Ashes of War / Gems
//   0xC0000000  Unknown/special
pub const GA_ITEM_TABLE_OFFSET: usize = 32;
pub const GA_ITEM_COUNT: usize = 0x1400;

pub fn read_u32_le(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(bytes[off..off+4].try_into().unwrap())
}

pub fn write_u32_le(bytes: &mut [u8], off: usize, val: u32) {
    bytes[off..off+4].copy_from_slice(&val.to_le_bytes());
}

pub fn read_character_name_utf16(bytes: &[u8], pgd_start: usize) -> String {
    let name_start = pgd_start + PGD_CHARACTER_NAME;
    let name_end = name_start + PGD_CHARACTER_NAME_LEN_BYTES;
    let mut wide: Vec<u16> = Vec::with_capacity(16);
    let mut i = name_start;
    while i + 1 < name_end {
        let ch = u16::from_le_bytes([bytes[i], bytes[i+1]]);
        if ch == 0 { break; }
        wide.push(ch);
        i += 2;
    }
    String::from_utf16_lossy(&wide)
}

/// Scans all 10 slots for a stat signature matching `current_stats` and
/// additionally verifies `expected_level`/`expected_runes` at the known offsets.
/// Returns (slot_idx, pgd_start_abs_offset).
pub fn find_character(
    bytes: &[u8],
    current_stats: &[u32; 8],
    expected_level: u32,
    expected_runes: u32,
) -> Option<(usize, usize)> {
    let mut sig: Vec<u8> = Vec::with_capacity(32);
    for s in current_stats { sig.extend(&s.to_le_bytes()); }

    for slot_idx in 0..NUM_SLOTS {
        let (slot_start, slot_end) = slot_data_range(slot_idx);
        if slot_end > bytes.len() { break; }

        let slot = &bytes[slot_start..slot_end];
        let mut pos = 0;
        while let Some(rel) = slot[pos..].windows(32).position(|w| w == sig.as_slice()) {
            let stat_abs = slot_start + pos + rel;
            let pgd_start = stat_abs - STATS_OFFSET_IN_PGD;
            let level_abs = stat_abs + 44;
            let runes_abs = stat_abs + 48;
            if level_abs + 4 <= bytes.len() && runes_abs + 4 <= bytes.len() {
                let lvl = read_u32_le(bytes, level_abs);
                let runes = read_u32_le(bytes, runes_abs);
                if lvl == expected_level && runes == expected_runes {
                    return Some((slot_idx, pgd_start));
                }
            }
            pos += rel + 1;
        }
    }
    None
}

/// Finds the first non-empty active slot by scanning all 10 for anything that
/// looks like a valid PlayerGameData (plausible stats + level = sum - 79).
/// Returns (slot_idx, pgd_start). Used by `read` when user hasn't pinned values.
pub fn find_any_active_character(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for slot_idx in 0..NUM_SLOTS {
        let (slot_start, slot_end) = slot_data_range(slot_idx);
        if slot_end > bytes.len() { break; }

        // Walk the GaItem table to find where PlayerGameData starts. Safer than
        // guessing: we know the table starts at slot_start + 32 and has exactly
        // 0x1400 variable-size entries.
        let mut pos = slot_start + GA_ITEM_TABLE_OFFSET;
        let mut ok = true;
        for _ in 0..GA_ITEM_COUNT {
            if pos + 8 > slot_end { ok = false; break; }
            let item_id = read_u32_le(bytes, pos + 4);
            let cat = item_id & 0xf0000000;
            pos += 8;
            if item_id != 0 && cat == 0 {
                pos += 13;
            } else if item_id != 0 && cat == 0x10000000 {
                pos += 8;
            }
            if pos > slot_end { ok = false; break; }
        }
        if !ok { continue; }

        // pos should now be at the start of PlayerGameData. Sanity-check it.
        let pgd_start = pos;
        if pgd_start + PGD_SIZE > slot_end { continue; }
        let stats: [u32; 8] = std::array::from_fn(|i|
            read_u32_le(bytes, pgd_start + STATS_OFFSET_IN_PGD + i * 4));
        let stat_sum: u32 = stats.iter().sum();
        let level = read_u32_le(bytes, pgd_start + PGD_LEVEL);
        // Plausibility: every stat 1..99, and level = sum - 79.
        let plausible = stats.iter().all(|&s| (1..=99).contains(&s))
            && level == stat_sum.saturating_sub(79)
            && (1..=713).contains(&level);
        if plausible {
            found.push((slot_idx, pgd_start));
        }
    }
    found
}

