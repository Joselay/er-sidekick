use serde::Serialize;

use crate::read::{Character, Item};

#[derive(Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Tip,
    Suggestion,
    Warning,
}

#[derive(Serialize, Clone)]
pub struct Recommendation {
    pub severity: Severity,
    pub title: String,
    pub detail: String,
}

pub fn advise(c: &Character) -> Vec<Recommendation> {
    let mut out = Vec::new();

    for rule in RULES {
        if let Some(r) = rule(c) {
            out.push(r);
        }
    }
    out
}

type Rule = fn(&Character) -> Option<Recommendation>;

const RULES: &[Rule] = &[
    rule_low_vigor,
    rule_rune_memory_lost,
    rule_missing_helm,
    rule_no_talismans_equipped,
    rule_talisman_pouch_available,
    rule_owned_but_unused_weapons,
    rule_arcane_dump_stat,
    rule_wondrous_physick,
    rule_flask_upgrade,
    rule_larval_tear,
];

fn find_item(items: &[Item], substring: &str) -> Option<Item> {
    items.iter().find(|i| i.name.as_deref().map(|n| n.contains(substring)).unwrap_or(false)).cloned()
}

fn any_equipped(slots: &[Option<Item>]) -> bool {
    slots.iter().any(|s| matches!(s, Some(i) if !matches!(i.name.as_deref(), Some("Unarmed") | None)))
}

// —————————————————————————————————————————————————————————————————————

fn rule_low_vigor(c: &Character) -> Option<Recommendation> {
    if c.level < 10 { return None; }
    // Rule of thumb: Vigor should at least match (level / 3 + 8) by this point.
    // A Samurai at Lv22 with Vigor 12 is very squishy — most bosses two-shot.
    let floor = (c.level / 3).saturating_add(8).min(40);
    if c.stats.vigor < floor {
        Some(Recommendation {
            severity: Severity::Warning,
            title: "Vigor is low for your level".to_string(),
            detail: format!(
                "Vigor {} at Level {} — the soft-cap guidance is {}+. Most bosses at this stage will two-shot you. \
                 Put your next {} level-ups into Vigor before any other stat.",
                c.stats.vigor, c.level, floor, floor - c.stats.vigor
            ),
        })
    } else {
        None
    }
}

fn rule_rune_memory_lost(c: &Character) -> Option<Recommendation> {
    // If you died with a lot of runes and haven't recovered them, remind you.
    // Threshold: memory > 5× current level (a meaningful pile).
    let threshold = (c.level as u32).saturating_mul(500).max(5000);
    if c.runes < 1000 && c.runes_memory > threshold {
        Some(Recommendation {
            severity: Severity::Warning,
            title: "Runes dropped at your last death site".to_string(),
            detail: format!(
                "You have {} runes in hand but {} sitting at your death spot. \
                 Go pick them up before dying again — death twice loses them permanently.",
                c.runes, c.runes_memory
            ),
        })
    } else {
        None
    }
}

fn rule_missing_helm(c: &Character) -> Option<Recommendation> {
    if c.equipped.head.is_some() { return None; }
    // Find any head armor the user owns.
    let head = c.inventory.armor.iter().find(|i| {
        i.name.as_deref().map(|n| {
            // Armor db uses generic "Head" for empty IDs; filter out.
            !n.is_empty() && n != "Head" && n != "Body" && n != "Arms" && n != "Legs"
                && (n.contains("Helm") || n.contains("Hood") || n.contains("Hat")
                    || n.contains("Crown") || n.contains("Mask") || n.contains("Cap"))
        }).unwrap_or(false)
    });
    head.map(|h| Recommendation {
        severity: Severity::Suggestion,
        title: "No helm equipped".to_string(),
        detail: format!(
            "You're unprotected up top. You own '{}' — equip it from the armor menu for free physical defense.",
            h.name.as_deref().unwrap_or("?")
        ),
    })
}

fn rule_no_talismans_equipped(c: &Character) -> Option<Recommendation> {
    let slots = &c.equipped.talismans;
    if slots.iter().any(|s| s.is_some()) { return None; }
    if c.inventory.talismans.is_empty() {
        return None; // no talismans owned — different rule handles pouch
    }
    let first = c.inventory.talismans.first()?;
    Some(Recommendation {
        severity: Severity::Suggestion,
        title: "No talismans equipped".to_string(),
        detail: format!(
            "You own {} talisman(s) but none are equipped. Start with '{}' — it's free defensive value.",
            c.inventory.talismans.len(),
            first.name.as_deref().unwrap_or("one of them")
        ),
    })
}

fn rule_talisman_pouch_available(c: &Character) -> Option<Recommendation> {
    let has_pouch = find_item(&c.inventory.key_items, "Talisman Pouch");
    if has_pouch.is_none() { return None; }
    // User has the pouch key item. If they own a pouch but have zero talismans equipped
    // AND no talismans owned, they should farm some early talismans.
    if c.inventory.talismans.is_empty() {
        Some(Recommendation {
            severity: Severity::Tip,
            title: "Talisman Pouch unused — no talismans owned yet".to_string(),
            detail: "You have the Talisman Pouch key item but no talismans to put in it. \
                     Good early talismans: Green Turtle Talisman (stamina, in Summonwater), \
                     Stalwart Horn Charm (poise), or any Scarseal/Soreseal for early stat boost.".to_string(),
        })
    } else { None }
}

fn rule_owned_but_unused_weapons(c: &Character) -> Option<Recommendation> {
    // If user owns a legendary but isn't wielding it, drop a hint about stat requirements.
    let legendaries = [
        ("Grafted Blade Greatsword", 40, 14, 12, 0, 0),
        ("Golden Halberd",          30, 14, 0,  0, 12),
        ("Sword of Night and Flame", 12, 12, 24, 24, 0),
    ];
    for (name, str_req, dex_req, int_req, fth_req, _arc_req) in legendaries {
        if find_item(&c.inventory.weapons, name).is_none() { continue; }
        let right_has = c.equipped.right_hand.iter().any(|s|
            s.as_ref().and_then(|i| i.name.as_deref()).map(|n| n == name).unwrap_or(false));
        let left_has = c.equipped.left_hand.iter().any(|s|
            s.as_ref().and_then(|i| i.name.as_deref()).map(|n| n == name).unwrap_or(false));
        if right_has || left_has { continue; }

        let meets = c.stats.strength >= str_req && c.stats.dexterity >= dex_req
            && c.stats.intelligence >= int_req && c.stats.faith >= fth_req;
        let msg = if meets {
            format!("You own '{}' and already meet its stat requirements \
                    (STR {} / DEX {} / INT {} / FTH {}). Try it.", name, str_req, dex_req, int_req, fth_req)
        } else {
            format!("You own '{}' but need STR {} / DEX {} / INT {} / FTH {} to wield it two-handed without penalty. \
                    Bank levels toward those stats if you want to use it.", name, str_req, dex_req, int_req, fth_req)
        };
        return Some(Recommendation {
            severity: Severity::Tip,
            title: format!("{} sitting in inventory", name),
            detail: msg,
        });
    }
    None
}

fn rule_arcane_dump_stat(c: &Character) -> Option<Recommendation> {
    // If Arcane is at or below the starting class value (8 for Samurai) and they have
    // no Arcane-scaling weapon equipped, let them know future levels shouldn't touch it.
    if c.class != "Samurai" { return None; }
    if c.stats.arcane > 10 { return None; }
    // Rivers of Blood, Frozen Needle at late game use arcane. But early, skip it.
    Some(Recommendation {
        severity: Severity::Tip,
        title: "Arcane is a dump stat for your build".to_string(),
        detail: "Your Samurai/Uchigatana build scales off DEX + ARC (bleed). Arcane is your tiebreaker stat — \
                 boosting it increases bleed proc rate. Consider 15-20 ARC around Level 30-40 for bleed builds. \
                 Until then, prioritize VIG > END > DEX.".to_string(),
    })
}

fn rule_wondrous_physick(c: &Character) -> Option<Recommendation> {
    let has_tears = c.inventory.key_items.iter().any(|i|
        i.name.as_deref().map(|n| n.contains("Crystal Tear")).unwrap_or(false));
    let has_physick = c.inventory.goods.iter().any(|i|
        i.name.as_deref().map(|n| n.contains("Flask of Wondrous Physick")).unwrap_or(false));
    if has_tears && !has_physick {
        Some(Recommendation {
            severity: Severity::Suggestion,
            title: "You have Crystal Tears but no Physick flask".to_string(),
            detail: "Get the Flask of Wondrous Physick from the Church of Elleh (north of the starting area, \
                     look for the cracked egg). Then mix two tears at any Site of Grace for a custom buff.".to_string(),
        })
    } else if has_tears && has_physick {
        // Note: can't check whether a tear is actively mixed (that's stored elsewhere).
        Some(Recommendation {
            severity: Severity::Tip,
            title: "Remember to mix your Crystal Tears".to_string(),
            detail: "At any Site of Grace: 'Mix Wondrous Physick' → pick 2 tears. \
                     Good combos for Samurai: Crimson Crystal Tear (+50% HP) + Greenspill (+stamina regen).".to_string(),
        })
    } else { None }
}

fn rule_flask_upgrade(c: &Character) -> Option<Recommendation> {
    // Find the highest flask upgrade the user owns.
    let flask = c.inventory.goods.iter()
        .filter_map(|i| {
            let n = i.name.as_deref()?;
            if !n.starts_with("Flask of Crimson Tears") && !n.starts_with("Flask of Cerulean Tears") { return None; }
            let plus = n.rfind('+').and_then(|p| n[p+1..].trim().parse::<u32>().ok()).unwrap_or(0);
            Some((plus, n.to_string()))
        })
        .max_by_key(|(p, _)| *p);
    let (level, _name) = flask?;
    if level < 12 {
        Some(Recommendation {
            severity: Severity::Tip,
            title: format!("Flask upgrade path: currently +{}", level),
            detail: "Golden Seeds upgrade flask charge count; Sacred Tears upgrade healing per charge (up to +12). \
                     Golden Seeds: branches on the Roundtable map. Sacred Tears: small churches.".to_string(),
        })
    } else { None }
}

fn rule_larval_tear(c: &Character) -> Option<Recommendation> {
    let has_larval = c.inventory.key_items.iter().any(|i|
        i.name.as_deref().map(|n| n.contains("Larval Tear")).unwrap_or(false));
    if has_larval {
        Some(Recommendation {
            severity: Severity::Tip,
            title: "You have a Larval Tear — respec available".to_string(),
            detail: "Rennala (Academy of Raya Lucaria boss, Carian Study Hall region) respecs stats in exchange for \
                     a Larval Tear. Use this if you want to fix stat-spread mistakes later.".to_string(),
        })
    } else { None }
}
