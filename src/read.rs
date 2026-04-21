use std::collections::HashMap;
use std::fs;
use std::process;

use serde::Serialize;

use crate::advise::{advise, Recommendation, Severity};
use crate::db;
use crate::save::{
    find_any_active_character, read_character_name_utf16, read_u32_le, COMMON_INVENTORY_SLOTS,
    EQUIP_DATA_OFFSET_FROM_PGD, EQUIP_INVENTORY_OFFSET_FROM_PGD, GA_ITEM_COUNT,
    GA_ITEM_TABLE_OFFSET, INVENTORY_ITEM_SIZE, KEY_INVENTORY_SLOTS, PGD_ARCHE_TYPE, PGD_GENDER,
    PGD_GIFT, PGD_LEVEL, PGD_SOULS, PGD_SOULS_MEMORY, STATS_OFFSET_IN_PGD,
};
use crate::slot_data_range;

#[derive(Serialize)]
pub struct Character {
    pub slot: usize,
    pub name: String,
    pub class: String,
    pub gender: String,
    pub gift: u8,
    pub level: u32,
    pub runes: u32,
    pub runes_memory: u32,
    pub stats: Stats,
    pub equipped: Equipped,
    pub inventory: Inventory,
    #[serde(default)]
    pub advice: Vec<Recommendation>,
}

#[derive(Serialize)]
pub struct Stats {
    pub vigor: u32,
    pub mind: u32,
    pub endurance: u32,
    pub strength: u32,
    pub dexterity: u32,
    pub intelligence: u32,
    pub faith: u32,
    pub arcane: u32,
}

#[derive(Serialize, Default)]
pub struct Equipped {
    pub right_hand: [Option<Item>; 3],
    pub left_hand: [Option<Item>; 3],
    pub arrows: [Option<Item>; 2],
    pub bolts: [Option<Item>; 2],
    pub head: Option<Item>,
    pub chest: Option<Item>,
    pub arms: Option<Item>,
    pub legs: Option<Item>,
    pub talismans: [Option<Item>; 4],
}

#[derive(Serialize, Clone)]
pub struct Item {
    pub name: Option<String>,
    pub item_id: u32,
    pub quantity: u32,
    pub category: &'static str,
}

#[derive(Serialize, Default)]
pub struct Inventory {
    pub common_distinct: u32,
    pub key_distinct: u32,
    pub weapons: Vec<Item>,
    pub armor: Vec<Item>,
    pub talismans: Vec<Item>,
    pub ashes_of_war: Vec<Item>,
    pub goods: Vec<Item>,
    pub key_items: Vec<Item>,
    pub other: Vec<Item>,
}

#[derive(Clone, Copy, Debug)]
enum ItemCategory {
    Weapon,
    Armor,
    Accessory,
    Goods,
    AshOfWar,
    Other,
    Empty,
}

fn categorize(item_id: u32) -> ItemCategory {
    if item_id == 0 || item_id == 0xffffffff {
        return ItemCategory::Empty;
    }
    match (item_id >> 28) & 0xf {
        0x0 => ItemCategory::Weapon,
        0x1 => ItemCategory::Armor,
        0x2 => ItemCategory::Accessory,
        0x8 => ItemCategory::AshOfWar,
        0xa | 0xb => ItemCategory::Goods,
        _ => ItemCategory::Other,
    }
}

fn category_label(c: ItemCategory) -> &'static str {
    match c {
        ItemCategory::Weapon => "weapon",
        ItemCategory::Armor => "armor",
        ItemCategory::Accessory => "talisman",
        ItemCategory::Goods => "goods",
        ItemCategory::AshOfWar => "ash_of_war",
        ItemCategory::Other => "other",
        ItemCategory::Empty => "empty",
    }
}

/// Map from gaitem handle → item_id. We insert each entry twice: once under
/// its full 32-bit handle, and once under its low-28-bit form. This is because
/// inventory tables tend to reference handles with their high nibble flag set
/// (e.g. 0x90000183), while EquipData slots sometimes reference the low bits
/// alone (e.g. 0x00000183).
fn parse_gaitem_table(bytes: &[u8], slot_start: usize, slot_end: usize) -> Option<HashMap<u32, u32>> {
    let mut pos = slot_start + GA_ITEM_TABLE_OFFSET;
    let mut map: HashMap<u32, u32> = HashMap::with_capacity(GA_ITEM_COUNT * 2);
    for _ in 0..GA_ITEM_COUNT {
        if pos + 8 > slot_end { return None; }
        let handle = read_u32_le(bytes, pos);
        let item_id = read_u32_le(bytes, pos + 4);
        pos += 8;
        let cat = item_id & 0xf0000000;
        if item_id != 0 && cat == 0 { pos += 13; }
        else if item_id != 0 && cat == 0x10000000 { pos += 8; }
        if pos > slot_end { return None; }
        if handle != 0 && item_id != 0 {
            map.insert(handle, item_id);
            map.entry(handle & 0x0fffffff).or_insert(item_id);
        }
    }
    Some(map)
}

fn class_name(arche_type: u8) -> String {
    match arche_type {
        0 => "Vagabond",
        1 => "Warrior",
        2 => "Hero",
        3 => "Bandit",
        4 => "Astrologer",
        5 => "Prophet",
        6 => "Confessor",
        7 => "Samurai",
        8 => "Prisoner",
        9 => "Wretch",
        _ => "Unknown",
    }
    .to_string()
}

fn gender_name(g: u8) -> String {
    if g == 0 { "Male" } else { "Female" }.to_string()
}

fn resolve_name(item_id: u32) -> Option<&'static str> {
    let cat = categorize(item_id);
    let base = item_id & 0x0fffffff;
    match cat {
        ItemCategory::Weapon => {
            let map = db::weapon_name::WEAPON_NAME.lock().ok()?;
            let stripped = base - (base % 100);
            map.get(&base).or_else(|| map.get(&stripped)).copied().filter(|s| !s.is_empty())
        }
        ItemCategory::Armor => {
            let map = db::armor_name::ARMOR_NAME.lock().ok()?;
            map.get(&base).copied().filter(|s| !s.is_empty())
        }
        ItemCategory::Accessory => {
            let map = db::accessory_name::ACCESSORY_NAME.lock().ok()?;
            map.get(&base).copied().filter(|s| !s.is_empty())
        }
        ItemCategory::Goods => {
            let map = db::item_name::ITEM_NAME.lock().ok()?;
            map.get(&base).copied().filter(|s| !s.is_empty())
        }
        ItemCategory::AshOfWar => {
            let map = db::aow_name::AOW_NAME.lock().ok()?;
            map.get(&base).copied().filter(|s| !s.is_empty())
        }
        _ => None,
    }
}

#[derive(Clone, Copy)]
enum SlotKind {
    HandWeapon,  // L/R hand armaments
    Ammo,        // arrow/bolt slot — only Weapon category items valid
    ArmorSlot,   // head/chest/arms/legs — only Armor category valid
    Talisman,    // only Accessory valid
}

impl SlotKind {
    fn accepts(self, cat: ItemCategory) -> bool {
        matches!(
            (self, cat),
            (SlotKind::HandWeapon, ItemCategory::Weapon)
                | (SlotKind::Ammo, ItemCategory::Weapon)
                | (SlotKind::ArmorSlot, ItemCategory::Armor)
                | (SlotKind::Talisman, ItemCategory::Accessory)
        )
    }
}

/// Resolve an equipped-slot handle. Equipped slots use a compact encoding
/// (`0x100 + running_index` for weapons/armor) vs the GaItem table's
/// (`0x00800000 + running_index`). Tries several forms, then requires the
/// resolved item's category to match the slot type — otherwise returns None
/// (avoids stale-memory false positives in unused ammo/talisman slots).
fn resolve_slot(handle: u32, kind: SlotKind, handle_to_item: &HashMap<u32, u32>) -> Option<Item> {
    if handle == 0 || handle == 0xffffffff { return None; }

    let candidates: [u32; 4] = [
        handle,
        handle | 0x00800000,
        handle.wrapping_sub(0x100) | 0x00800000,
        handle & 0x0fffffff,
    ];

    let item_id = candidates.iter()
        .find_map(|h| handle_to_item.get(h).copied())
        .unwrap_or(handle);
    let cat = categorize(item_id);

    if !kind.accepts(cat) {
        // Slot has a value, but resolution doesn't match slot's expected
        // category. Return a placeholder so the user sees *something* and can
        // tell the slot is non-empty.
        return Some(Item {
            name: None,
            item_id: handle,
            quantity: 1,
            category: "unresolved",
        });
    }

    Some(Item {
        name: resolve_name(item_id).map(|s| s.to_string()),
        item_id,
        quantity: 1,
        category: category_label(cat),
    })
}

// EquipData stores left/right hands and arrows/bolts INTERLEAVED in the save,
// even though the struct view presents them as separate arrays. Layout
// confirmed empirically against a live save:
//   +0  L[0], +4  R[0], +8  L[1], +12 R[1], +16 L[2], +20 R[2],
//   +24 arrow[0], +28 bolt[0], +32 arrow[1], +36 bolt[1],
//   +40 _unk,
//   +44 head, +48 chest, +52 arms, +56 legs,
//   +60 _unk,
//   +64 talisman[0], +68 [1], +72 [2], +76 [3],
//   +80..+87 trailing unk (8 bytes)
fn read_equipped(
    bytes: &[u8],
    pgd_start: usize,
    handle_to_item: &HashMap<u32, u32>,
) -> Equipped {
    let base = pgd_start + EQUIP_DATA_OFFSET_FROM_PGD;
    let at = |off: usize| read_u32_le(bytes, base + off);

    let w = SlotKind::HandWeapon;
    let a = SlotKind::Ammo;
    let ar = SlotKind::ArmorSlot;
    let t = SlotKind::Talisman;

    Equipped {
        left_hand: [
            resolve_slot(at(0),  w, handle_to_item),
            resolve_slot(at(8),  w, handle_to_item),
            resolve_slot(at(16), w, handle_to_item),
        ],
        right_hand: [
            resolve_slot(at(4),  w, handle_to_item),
            resolve_slot(at(12), w, handle_to_item),
            resolve_slot(at(20), w, handle_to_item),
        ],
        arrows: [
            resolve_slot(at(24), a, handle_to_item),
            resolve_slot(at(32), a, handle_to_item),
        ],
        bolts: [
            resolve_slot(at(28), a, handle_to_item),
            resolve_slot(at(36), a, handle_to_item),
        ],
        head:  resolve_slot(at(44), ar, handle_to_item),
        chest: resolve_slot(at(48), ar, handle_to_item),
        arms:  resolve_slot(at(52), ar, handle_to_item),
        legs:  resolve_slot(at(56), ar, handle_to_item),
        talismans: [
            resolve_slot(at(64), t, handle_to_item),
            resolve_slot(at(68), t, handle_to_item),
            resolve_slot(at(72), t, handle_to_item),
            resolve_slot(at(76), t, handle_to_item),
        ],
    }
}

fn read_inventory(
    bytes: &[u8],
    pgd_start: usize,
    slot_end: usize,
    handle_to_item: &HashMap<u32, u32>,
) -> Option<Inventory> {
    let inv_start = pgd_start + EQUIP_INVENTORY_OFFSET_FROM_PGD;
    let common_count_off = inv_start;
    let common_table_start = inv_start + 4;
    let key_count_off = common_table_start + COMMON_INVENTORY_SLOTS * INVENTORY_ITEM_SIZE;
    let key_table_start = key_count_off + 4;

    if key_table_start + KEY_INVENTORY_SLOTS * INVENTORY_ITEM_SIZE + 8 > slot_end {
        return None;
    }

    let common_distinct = read_u32_le(bytes, common_count_off);
    let key_distinct = read_u32_le(bytes, key_count_off);

    let mut inv = Inventory {
        common_distinct,
        key_distinct,
        ..Default::default()
    };

    let collect = |table_start: usize, slots: usize, is_key: bool, inv: &mut Inventory| {
        for i in 0..slots {
            let off = table_start + i * INVENTORY_ITEM_SIZE;
            let handle = read_u32_le(bytes, off);
            let quantity = read_u32_le(bytes, off + 4);
            if handle == 0 || handle == 0xffffffff || quantity == 0 { continue; }

            let item_id = handle_to_item.get(&handle).copied().unwrap_or(handle);
            let cat = categorize(item_id);
            let item = Item {
                name: resolve_name(item_id).map(|s| s.to_string()),
                item_id,
                quantity,
                category: category_label(cat),
            };
            if is_key {
                inv.key_items.push(item);
                continue;
            }
            match cat {
                ItemCategory::Weapon => inv.weapons.push(item),
                ItemCategory::Armor => inv.armor.push(item),
                ItemCategory::Accessory => inv.talismans.push(item),
                ItemCategory::AshOfWar => inv.ashes_of_war.push(item),
                ItemCategory::Goods => inv.goods.push(item),
                _ => inv.other.push(item),
            }
        }
    };

    collect(common_table_start, COMMON_INVENTORY_SLOTS, false, &mut inv);
    collect(key_table_start, KEY_INVENTORY_SLOTS, true, &mut inv);

    for bucket in [
        &mut inv.weapons, &mut inv.armor, &mut inv.talismans, &mut inv.ashes_of_war,
        &mut inv.goods, &mut inv.key_items, &mut inv.other,
    ] {
        bucket.sort_by(|a, b| {
            a.name.as_deref().unwrap_or("").cmp(b.name.as_deref().unwrap_or(""))
                .then(a.item_id.cmp(&b.item_id))
        });
    }

    Some(inv)
}

fn gather_character(bytes: &[u8], slot_idx: usize, pgd_start: usize) -> Option<Character> {
    let (slot_start, slot_end) = slot_data_range(slot_idx);
    let stats_arr: [u32; 8] =
        std::array::from_fn(|i| read_u32_le(bytes, pgd_start + STATS_OFFSET_IN_PGD + i * 4));
    let handle_to_item = parse_gaitem_table(bytes, slot_start, slot_end)?;


    let mut character = Character {
        slot: slot_idx,
        name: read_character_name_utf16(bytes, pgd_start),
        class: class_name(bytes[pgd_start + PGD_ARCHE_TYPE]),
        gender: gender_name(bytes[pgd_start + PGD_GENDER]),
        gift: bytes[pgd_start + PGD_GIFT],
        level: read_u32_le(bytes, pgd_start + PGD_LEVEL),
        runes: read_u32_le(bytes, pgd_start + PGD_SOULS),
        runes_memory: read_u32_le(bytes, pgd_start + PGD_SOULS_MEMORY),
        stats: Stats {
            vigor: stats_arr[0], mind: stats_arr[1], endurance: stats_arr[2],
            strength: stats_arr[3], dexterity: stats_arr[4], intelligence: stats_arr[5],
            faith: stats_arr[6], arcane: stats_arr[7],
        },
        equipped: read_equipped(bytes, pgd_start, &handle_to_item),
        inventory: read_inventory(bytes, pgd_start, slot_end, &handle_to_item).unwrap_or_default(),
        advice: Vec::new(),
    };
    character.advice = advise(&character);
    Some(character)
}

pub fn run(input: &str, as_json: bool) {
    let bytes = match fs::read(input) {
        Ok(b) => b,
        Err(e) => { eprintln!("read {}: {}", input, e); process::exit(1); }
    };
    if &bytes[0..4] != b"BND4" {
        eprintln!("not a BND4 save file (magic was {:?})", &bytes[0..4]);
        process::exit(1);
    }
    let found = find_any_active_character(&bytes);
    if found.is_empty() {
        eprintln!("no active characters found.");
        process::exit(1);
    }
    let characters: Vec<Character> = found.iter()
        .filter_map(|(slot, pgd)| gather_character(&bytes, *slot, *pgd))
        .collect();

    if as_json {
        let json = serde_json::to_string_pretty(&characters).unwrap();
        println!("{}", json);
    } else {
        for (i, c) in characters.iter().enumerate() {
            if i > 0 { println!("\n"); }
            print_character(c);
        }
    }
}

fn fmt_item(item: &Option<Item>) -> String {
    match item {
        None => "—".to_string(),
        Some(i) => match &i.name {
            Some(n) if !n.is_empty() => n.clone(),
            _ => format!("<0x{:08x}>", i.item_id),
        },
    }
}

fn print_character(c: &Character) {
    println!("═══════════════════════════════════════════════════");
    println!(" Slot {}: {}", c.slot, c.name);
    println!("═══════════════════════════════════════════════════");
    println!("  Class:   {}   Gender: {}   Gift: {}", c.class, c.gender, c.gift);
    println!("  Level:   {}", c.level);
    println!("  Runes:   {} held  ({} at death memory)", c.runes, c.runes_memory);
    println!();
    println!("  Stats:");
    println!("    Vigor         {:>3}", c.stats.vigor);
    println!("    Mind          {:>3}", c.stats.mind);
    println!("    Endurance     {:>3}", c.stats.endurance);
    println!("    Strength      {:>3}", c.stats.strength);
    println!("    Dexterity     {:>3}", c.stats.dexterity);
    println!("    Intelligence  {:>3}", c.stats.intelligence);
    println!("    Faith         {:>3}", c.stats.faith);
    println!("    Arcane        {:>3}", c.stats.arcane);

    println!();
    println!("  Equipped:");
    println!("    R-Hand:   [1] {}", fmt_item(&c.equipped.right_hand[0]));
    println!("              [2] {}", fmt_item(&c.equipped.right_hand[1]));
    println!("              [3] {}", fmt_item(&c.equipped.right_hand[2]));
    println!("    L-Hand:   [1] {}", fmt_item(&c.equipped.left_hand[0]));
    println!("              [2] {}", fmt_item(&c.equipped.left_hand[1]));
    println!("              [3] {}", fmt_item(&c.equipped.left_hand[2]));
    println!("    Arrows:   [1] {}", fmt_item(&c.equipped.arrows[0]));
    println!("              [2] {}", fmt_item(&c.equipped.arrows[1]));
    println!("    Bolts:    [1] {}", fmt_item(&c.equipped.bolts[0]));
    println!("              [2] {}", fmt_item(&c.equipped.bolts[1]));
    println!("    Head:         {}", fmt_item(&c.equipped.head));
    println!("    Chest:        {}", fmt_item(&c.equipped.chest));
    println!("    Arms:         {}", fmt_item(&c.equipped.arms));
    println!("    Legs:         {}", fmt_item(&c.equipped.legs));
    println!("    Talismans:[1] {}", fmt_item(&c.equipped.talismans[0]));
    println!("              [2] {}", fmt_item(&c.equipped.talismans[1]));
    println!("              [3] {}", fmt_item(&c.equipped.talismans[2]));
    println!("              [4] {}", fmt_item(&c.equipped.talismans[3]));

    println!();
    println!(
        "  Inventory: {} common / {} key distinct items",
        c.inventory.common_distinct, c.inventory.key_distinct
    );
    print_bucket("Weapons",      &c.inventory.weapons);
    print_bucket("Armor",        &c.inventory.armor);
    print_bucket("Talismans",    &c.inventory.talismans);
    print_bucket("Ashes of War", &c.inventory.ashes_of_war);
    print_bucket("Goods",        &c.inventory.goods);
    print_bucket("Key items",    &c.inventory.key_items);
    if !c.inventory.other.is_empty() {
        print_bucket("Other",    &c.inventory.other);
    }

    if !c.advice.is_empty() {
        println!();
        println!("═══════════════════════════════════════════════════");
        println!(" Sidekick says:");
        println!("═══════════════════════════════════════════════════");
        for r in &c.advice {
            let tag = match r.severity {
                Severity::Tip => "💡 TIP",
                Severity::Suggestion => "👉 SUGGESTION",
                Severity::Warning => "⚠️  WARNING",
            };
            println!();
            println!("  {}  {}", tag, r.title);
            for line in wrap(&r.detail, 78, "      ") {
                println!("{}", line);
            }
        }
    }
}

fn wrap(text: &str, width: usize, indent: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::from(indent);
    for word in text.split_whitespace() {
        if line.len() + word.len() + 1 > width {
            out.push(std::mem::replace(&mut line, String::from(indent)));
        }
        if line.len() > indent.len() { line.push(' '); }
        line.push_str(word);
    }
    if line.len() > indent.len() { out.push(line); }
    out
}

fn print_bucket(title: &str, items: &[Item]) {
    if items.is_empty() { return; }
    println!();
    println!("  {} ({}):", title, items.len());
    let preview_limit = 40;
    for it in items.iter().take(preview_limit) {
        let name = match &it.name {
            Some(n) if !n.is_empty() => n.clone(),
            _ => format!("<0x{:08x}>", it.item_id),
        };
        if it.quantity > 1 {
            println!("    {:<48} x{:<5}  (0x{:08x})", name, it.quantity, it.item_id);
        } else {
            println!("    {:<48}         (0x{:08x})", name, it.item_id);
        }
    }
    if items.len() > preview_limit {
        println!("    ... and {} more", items.len() - preview_limit);
    }
}
