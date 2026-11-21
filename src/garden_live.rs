//! Live Chao read from the headless recompiled game ([`crate::recomp::Worker`]).
//! The game's own ChaoMain runs the AI; this only reads its state at frame
//! boundaries. Offsets are from the GSNE8P ChaoMain code (chaowk = task->twp at
//! task+0x34; ALBHV behaviour block at chaowk+0x1B4) and were checked against
//! a headless garden run through the bridge (2026-11-21).
use crate::recomp::Worker;
use std::io;

const TASK_TWP: u32 = 0x34;
const WK_POS: u32 = 0x14;
const WK_BHV: u32 = 0x1B4;
const BHV_MODE: u32 = 0x2;
const BHV_TIMER: u32 = 0xC;
const BHV_INDEX: u32 = 0x284;
const BHV_LIST: u32 = 0x288;
/// Motion number, as read by GetMotionNum 0x8002CB90.
const WK_MOTION: u32 = 0xB0;
/// chaowk->pParamGC; its +0xD5 headgear byte is what al_draw_B (ChaoMain 0x8990) draws.
const WK_PARAM: u32 = 0x5C;
const PARAM_HAT: u32 = 0xD5;
/// OSModuleInfo list head; entry id +0, next +4, section table +0x10.
const MODULE_LIST: u32 = 0x8000_30C8;
const CHAOMAIN: u32 = 0x3C;
/// Task slot table: capacity r32(sec4+0x81E0+4c); slot sec6+0x44424+c*0x1200+i*0x48, task at +0x3C.
const CAP_TABLE: u32 = 0x81E0;
const SLOTS: u32 = 0x4_4424;
const CATEGORY_STRIDE: u32 = 0x1200;
const SLOT_SIZE: u32 = 0x48;
const SLOT_TASK: u32 = 0x3C;
/// Category 0 = Chao (eggs in category 1 move there on hatching).
const CAT_CHAO: u32 = 0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveChao {
    pub task: u32,
    pub pos: [f32; 3],
    /// Guest address of the running ALBHV_* function.
    pub behaviour: u32,
    pub mode: u16,
    pub timer: i32,
    pub motion: u32,
    /// Headgear id (0 = none).
    pub hat: u8,
}

fn be32(b: &[u8], at: u32) -> u32 {
    let i = at as usize;
    u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// Decode a Chao from its chaowk bytes (at least `WK_BHV + BHV_LIST + 4 * 16`).
fn decode(task: u32, wk: &[u8], behaviour: u32, hat: u8) -> LiveChao {
    let f = |at| f32::from_bits(be32(wk, at));
    let b = WK_BHV;
    LiveChao {
        task,
        pos: [f(WK_POS), f(WK_POS + 4), f(WK_POS + 8)],
        behaviour,
        mode: u16::from_be_bytes([wk[(b + BHV_MODE) as usize], wk[(b + BHV_MODE + 1) as usize]]),
        timer: be32(wk, b + BHV_TIMER) as i32,
        motion: be32(wk, WK_MOTION),
        hat,
    }
}

/// Read one Chao task's live state.
pub fn read_chao(w: &mut Worker, task: u32) -> io::Result<LiveChao> {
    let wk_addr = w.r32(task + TASK_TWP)?;
    let wk = w.read(wk_addr, WK_BHV + BHV_LIST)?;
    let idx = be32(&wk, WK_BHV + BHV_INDEX);
    let behaviour = w.r32(wk_addr + WK_BHV + BHV_LIST + 4 * idx)?;
    let hat = w.read(be32(&wk, WK_PARAM) + PARAM_HAT, 1)?[0];
    Ok(decode(task, &wk, behaviour, hat))
}

/// Run-time base of ChaoMain section `sec`, found through the game's module list.
pub fn chaomain_section(w: &mut Worker, sec: u32) -> io::Result<u32> {
    let mut m = w.r32(MODULE_LIST)?;
    while m != 0 {
        if w.r32(m)? == CHAOMAIN {
            let table = w.r32(m + 0x10)?;
            return Ok(w.r32(table + 8 * sec)? & !1);
        }
        m = w.r32(m + 4)?;
    }
    Err(io::Error::other("ChaoMain not linked"))
}

/// Task pointers of every live Chao (category 0 slots).
pub fn chao_tasks(w: &mut Worker) -> io::Result<Vec<u32>> {
    let (sec4, sec6) = (chaomain_section(w, 4)?, chaomain_section(w, 6)?);
    let cap = w.r32(sec4 + CAP_TABLE + 4 * CAT_CHAO)?;
    let base = sec6 + SLOTS + CAT_CHAO * CATEGORY_STRIDE;
    let mut tasks = Vec::new();
    for i in 0..cap {
        let t = w.r32(base + i * SLOT_SIZE + SLOT_TASK)?;
        if t != 0 {
            tasks.push(t);
        }
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_position_mode_and_timer() {
        let mut wk = vec![0u8; (WK_BHV + BHV_LIST) as usize];
        wk[0x14..0x18].copy_from_slice(&(-58.0f32).to_be_bytes());
        wk[0x18..0x1C].copy_from_slice(&2.0f32.to_be_bytes());
        wk[0x1C..0x20].copy_from_slice(&5.0f32.to_be_bytes());
        let b = WK_BHV as usize;
        wk[b + 2..b + 4].copy_from_slice(&3u16.to_be_bytes());
        wk[b + 0xC..b + 0x10].copy_from_slice(&85i32.to_be_bytes());
        wk[0xB0..0xB4].copy_from_slice(&205u32.to_be_bytes());
        let c = decode(0x8100_0000, &wk, 0x8050_0000, 9);
        assert_eq!(c.pos, [-58.0, 2.0, 5.0]);
        assert_eq!(c.hat, 9);
        assert_eq!(
            (c.mode, c.timer, c.behaviour, c.motion),
            (3, 85, 0x8050_0000, 205)
        );
    }
}
