//! Live memory editor for Elden Ring.
//!
//! Attaches to a running `eldenring.exe`, AOB-scans for the `GameDataMan`
//! static pointer slot, walks a 2-level dereference chain, and reads/writes
//! the in-memory `CharacterStats` struct so changes show up in-game without
//! a restart.
//!
//! Pointer chain (from veeenu/eldenring-practice-tool):
//!   character_stats_addr = *(*game_data_man + 0x8) + 0x3c
//!
//! CharacterStats layout in memory (56 bytes, tightly packed i32):
//!   +0x00 vigor  +0x04 mind  +0x08 endurance  +0x0c strength
//!   +0x10 dex    +0x14 int   +0x18 faith      +0x1c arcane
//!   +0x20..0x2b  padding (3 u32)
//!   +0x2c level  +0x30 runes (held)  +0x34 runes_total (memory)
//!
//! This is a DIFFERENT layout from the save file — do not reuse save offsets.
//!
//! Writing a stat updates the game's stat menu immediately. Derived values
//! (HP, FP, stamina, attack power) only recalc on triggers like opening the
//! equip menu or resting at a site of grace.

#![cfg(windows)]

use std::mem::{size_of, zeroed};

use windows::Win32::Foundation::{CloseHandle, HANDLE, HMODULE};
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{EnumProcessModules, GetModuleInformation, MODULEINFO};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ,
    PROCESS_VM_WRITE,
};

const PROC_NAME: &str = "eldenring.exe";

// mov rax, [rip+disp32] / test rax,rax / jz short / mov rax,[rax+58] / ret / ret
// disp32 is at match+3, next-instruction is at match+7.
const GAMEDATAMAN_PATTERN: &str =
    "48 8B 05 ?? ?? ?? ?? 48 85 C0 74 05 48 8B 40 58 C3 C3";
const RIP_DISP_OFFSET: usize = 3;
const RIP_NEXT_OFFSET: usize = 7;

const CHAIN_OFFSETS: [usize; 2] = [0x8, 0x3c];

pub struct Attached {
    proc: HANDLE,
    module_base: usize,
    module_size: usize,
}

impl Drop for Attached {
    fn drop(&mut self) {
        unsafe { let _ = CloseHandle(self.proc); }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct LiveStats {
    pub vigor: i32,
    pub mind: i32,
    pub endurance: i32,
    pub strength: i32,
    pub dexterity: i32,
    pub intelligence: i32,
    pub faith: i32,
    pub arcane: i32,
    pub _pad: [u32; 3],
    pub level: i32,
    pub runes: i32,
    pub runes_total: i32,
}

pub struct Session {
    attached: Attached,
    stats_addr: usize,
}

fn find_pid(name: &str) -> Option<u32> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..zeroed()
        };
        let target = name.to_lowercase();
        let mut result = None;
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if exe.to_lowercase() == target {
                    result = Some(entry.th32ProcessID);
                    break;
                }
                if Process32NextW(snap, &mut entry).is_err() { break; }
            }
        }
        let _ = CloseHandle(snap);
        result
    }
}

fn open_and_get_main_module(pid: u32) -> Result<Attached, String> {
    unsafe {
        let access = PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION
            | PROCESS_QUERY_INFORMATION;
        let proc = OpenProcess(access, false, pid)
            .map_err(|e| format!("OpenProcess({pid}): {e}"))?;

        let mut modules = [HMODULE::default(); 1];
        let mut needed: u32 = 0;
        EnumProcessModules(
            proc,
            modules.as_mut_ptr(),
            size_of::<HMODULE>() as u32,
            &mut needed,
        )
        .map_err(|e| format!("EnumProcessModules: {e}"))?;

        let mut info = MODULEINFO::default();
        GetModuleInformation(proc, modules[0], &mut info, size_of::<MODULEINFO>() as u32)
            .map_err(|e| format!("GetModuleInformation: {e}"))?;

        Ok(Attached {
            proc,
            module_base: info.lpBaseOfDll as usize,
            module_size: info.SizeOfImage as usize,
        })
    }
}

fn read_bytes(a: &Attached, addr: usize, buf: &mut [u8]) -> bool {
    unsafe {
        ReadProcessMemory(a.proc, addr as _, buf.as_mut_ptr() as _, buf.len(), None).is_ok()
    }
}

fn write_bytes(a: &Attached, addr: usize, buf: &[u8]) -> bool {
    unsafe {
        WriteProcessMemory(a.proc, addr as _, buf.as_ptr() as _, buf.len(), None).is_ok()
    }
}

fn read_usize(a: &Attached, addr: usize) -> Option<usize> {
    let mut v = [0u8; 8];
    if read_bytes(a, addr, &mut v) { Some(usize::from_le_bytes(v)) } else { None }
}

fn parse_pattern(s: &str) -> (Vec<u8>, Vec<bool>) {
    let mut bytes = Vec::new();
    let mut mask = Vec::new();
    for tok in s.split_whitespace() {
        if tok == "??" || tok == "?" {
            bytes.push(0);
            mask.push(false);
        } else {
            bytes.push(u8::from_str_radix(tok, 16).expect("bad hex in AOB"));
            mask.push(true);
        }
    }
    (bytes, mask)
}

fn find_in_chunk(chunk: &[u8], pat: &[u8], mask: &[bool]) -> Option<usize> {
    if chunk.len() < pat.len() { return None; }
    'outer: for i in 0..=chunk.len() - pat.len() {
        for j in 0..pat.len() {
            if mask[j] && chunk[i + j] != pat[j] {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

/// Scan the process's main module for an AOB pattern. Reads in 1 MiB chunks
/// with overlap to catch patterns straddling a boundary. Unmapped regions
/// are skipped.
fn scan_module(a: &Attached, pattern: &str) -> Option<usize> {
    let (pat, mask) = parse_pattern(pattern);
    let pat_len = pat.len();
    const CHUNK: usize = 1024 * 1024;
    let overlap = pat_len.saturating_sub(1);

    let mut buf = vec![0u8; CHUNK];
    let mut off = 0usize;
    while off < a.module_size {
        let to_read = CHUNK.min(a.module_size - off);
        buf.resize(to_read, 0);
        if read_bytes(a, a.module_base + off, &mut buf) {
            if let Some(i) = find_in_chunk(&buf, &pat, &mask) {
                return Some(a.module_base + off + i);
            }
        }
        if off + to_read >= a.module_size { break; }
        off += to_read - overlap;
    }
    None
}

fn resolve_rip(a: &Attached, match_addr: usize) -> Option<usize> {
    let mut disp = [0u8; 4];
    if !read_bytes(a, match_addr + RIP_DISP_OFFSET, &mut disp) { return None; }
    let disp = i32::from_le_bytes(disp) as isize;
    let next_ip = (match_addr + RIP_NEXT_OFFSET) as isize;
    Some((next_ip + disp) as usize)
}

fn eval_chain(a: &Attached, base: usize, offsets: &[usize]) -> Option<usize> {
    offsets.iter().try_fold(base, |addr, &offs| {
        read_usize(a, addr).map(|v| v + offs)
    })
}

impl Session {
    pub fn attach() -> Result<Self, String> {
        let pid = find_pid(PROC_NAME)
            .ok_or_else(|| format!("{PROC_NAME} not running"))?;
        let attached = open_and_get_main_module(pid)?;

        let hit = scan_module(&attached, GAMEDATAMAN_PATTERN)
            .ok_or("GameDataMan signature not found — game version may be unsupported")?;
        let static_slot = resolve_rip(&attached, hit)
            .ok_or("failed to resolve RIP-relative disp for GameDataMan")?;

        let stats_addr = eval_chain(&attached, static_slot, &CHAIN_OFFSETS)
            .ok_or("pointer chain walk failed — is a character loaded in-game?")?;

        Ok(Session { attached, stats_addr })
    }

    pub fn read_stats(&self) -> Option<LiveStats> {
        let mut buf = [0u8; size_of::<LiveStats>()];
        if !read_bytes(&self.attached, self.stats_addr, &mut buf) { return None; }
        Some(unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const LiveStats) })
    }

    pub fn write_stats(&self, s: &LiveStats) -> bool {
        let bytes = unsafe {
            std::slice::from_raw_parts(s as *const _ as *const u8, size_of::<LiveStats>())
        };
        write_bytes(&self.attached, self.stats_addr, bytes)
    }

    pub fn apply_edits(&self, edits: &[(String, StatEdit)]) -> Result<LiveStats, String> {
        let mut s = self.read_stats().ok_or("failed to read current stats")?;
        for (name, edit) in edits {
            let field: &mut i32 = match name.as_str() {
                "vigor" | "vig" | "vgr" => &mut s.vigor,
                "mind" | "mnd" => &mut s.mind,
                "endurance" | "end" | "edr" => &mut s.endurance,
                "strength" | "str" | "stg" => &mut s.strength,
                "dexterity" | "dex" | "dxt" => &mut s.dexterity,
                "intelligence" | "int" | "itl" => &mut s.intelligence,
                "faith" | "fth" | "fai" => &mut s.faith,
                "arcane" | "arc" | "arn" => &mut s.arcane,
                "level" | "lvl" => &mut s.level,
                "runes" => &mut s.runes,
                "runes_total" | "runes_memory" => &mut s.runes_total,
                other => return Err(format!("unknown stat: {other}")),
            };
            edit.apply(field);
        }

        // Keep level consistent with stat sum so the game/save remain valid.
        // Formula: level = sum(stats) - 79 (Elden Ring reserves 79 base points).
        let stats_sum = s.vigor + s.mind + s.endurance + s.strength
            + s.dexterity + s.intelligence + s.faith + s.arcane;
        let expected_level = (stats_sum - 79).max(1);
        if s.level != expected_level {
            s.level = expected_level;
        }

        if !self.write_stats(&s) { return Err("WriteProcessMemory failed".into()); }
        Ok(s)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum StatEdit {
    Set(i32),
    Add(i32),
}

impl StatEdit {
    fn apply(&self, field: &mut i32) {
        match *self {
            StatEdit::Set(v) => *field = v,
            StatEdit::Add(d) => *field = field.saturating_add(d),
        }
    }
}

pub fn parse_edits(s: &str) -> Result<Vec<(String, StatEdit)>, String> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() { continue; }
        let eq = part.find('=').ok_or_else(|| format!("expected key=value in '{part}'"))?;
        let key = part[..eq].trim().to_lowercase();
        let val = part[eq+1..].trim();
        let edit = if let Some(rest) = val.strip_prefix('+') {
            StatEdit::Add(rest.parse().map_err(|_| format!("bad number '{rest}'"))?)
        } else if let Some(rest) = val.strip_prefix('-') {
            StatEdit::Add(-rest.parse::<i32>().map_err(|_| format!("bad number '{rest}'"))?)
        } else {
            StatEdit::Set(val.parse().map_err(|_| format!("bad number '{val}'"))?)
        };
        out.push((key, edit));
    }
    Ok(out)
}

// Public CLI entry points.

pub fn run_read(as_json: bool) {
    match Session::attach() {
        Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        Ok(sess) => {
            let s = match sess.read_stats() {
                Some(v) => v,
                None => { eprintln!("read_stats: RPM failed"); std::process::exit(1); }
            };
            if as_json {
                println!("{}", serde_stats(&s));
            } else {
                print_stats(&s);
            }
        }
    }
}

pub fn run_set(edit_str: &str) {
    let edits = match parse_edits(edit_str) {
        Ok(v) => v,
        Err(e) => { eprintln!("parse: {e}"); std::process::exit(1); }
    };
    if edits.is_empty() {
        eprintln!("no edits supplied");
        std::process::exit(1);
    }
    let sess = match Session::attach() {
        Ok(s) => s,
        Err(e) => { eprintln!("{e}"); std::process::exit(1); }
    };
    let before = sess.read_stats();
    match sess.apply_edits(&edits) {
        Ok(after) => {
            println!("=== Live edit applied ===");
            if let Some(b) = before { print_diff(&b, &after); }
            else { print_stats(&after); }
        }
        Err(e) => { eprintln!("apply: {e}"); std::process::exit(1); }
    }
}

fn print_stats(s: &LiveStats) {
    println!("  Vigor         {:>3}", s.vigor);
    println!("  Mind          {:>3}", s.mind);
    println!("  Endurance     {:>3}", s.endurance);
    println!("  Strength      {:>3}", s.strength);
    println!("  Dexterity     {:>3}", s.dexterity);
    println!("  Intelligence  {:>3}", s.intelligence);
    println!("  Faith         {:>3}", s.faith);
    println!("  Arcane        {:>3}", s.arcane);
    println!("  Level         {:>3}", s.level);
    println!("  Runes (held)  {}", s.runes);
    println!("  Runes (memory){:>3}", s.runes_total);
}

fn print_diff(b: &LiveStats, a: &LiveStats) {
    let rows: [(&str, i32, i32); 11] = [
        ("Vigor",        b.vigor, a.vigor),
        ("Mind",         b.mind, a.mind),
        ("Endurance",    b.endurance, a.endurance),
        ("Strength",     b.strength, a.strength),
        ("Dexterity",    b.dexterity, a.dexterity),
        ("Intelligence", b.intelligence, a.intelligence),
        ("Faith",        b.faith, a.faith),
        ("Arcane",       b.arcane, a.arcane),
        ("Level",        b.level, a.level),
        ("Runes",        b.runes, a.runes),
        ("Runes memory", b.runes_total, a.runes_total),
    ];
    for (name, bv, av) in rows {
        if bv != av { println!("  {:<14} {} -> {}", name, bv, av); }
        else        { println!("  {:<14} {}", name, av); }
    }
}

fn serde_stats(s: &LiveStats) -> String {
    format!(
        "{{\"vigor\":{},\"mind\":{},\"endurance\":{},\"strength\":{},\"dexterity\":{},\"intelligence\":{},\"faith\":{},\"arcane\":{},\"level\":{},\"runes\":{},\"runes_total\":{}}}",
        s.vigor, s.mind, s.endurance, s.strength, s.dexterity, s.intelligence, s.faith, s.arcane, s.level, s.runes, s.runes_total
    )
}
