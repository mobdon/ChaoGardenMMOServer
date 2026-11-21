//! Live Chao read from the headless recompiled game ([`crate::recomp::Worker`]).
//! The game's own ChaoMain runs the AI; this only reads its state at frame
//! boundaries. Offsets are from the GSNE8P ChaoMain code (chaowk = task->twp at
//! task+0x34; ALBHV behaviour block at chaowk+0x1B4) and were checked against
//! a headless garden trace (2026-11-21).
use crate::recomp::Worker;
use std::io;

const TASK_TWP: u32 = 0x34;
const WK_POS: u32 = 0x14;
const WK_BHV: u32 = 0x1B4;
const BHV_MODE: u32 = 0x2;
const BHV_TIMER: u32 = 0xC;
const BHV_INDEX: u32 = 0x284;
const BHV_LIST: u32 = 0x288;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveChao {
    pub task: u32,
    pub pos: [f32; 3],
    /// Guest address of the running ALBHV_* function.
    pub behaviour: u32,
    pub mode: u16,
    pub timer: i32,
}

fn be32(b: &[u8], at: u32) -> u32 {
    let i = at as usize;
    u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// Decode a Chao from its chaowk bytes (at least `WK_BHV + BHV_LIST + 4 * 16`).
fn decode(task: u32, wk: &[u8], behaviour: u32) -> LiveChao {
    let f = |at| f32::from_bits(be32(wk, at));
    let b = WK_BHV;
    LiveChao {
        task,
        pos: [f(WK_POS), f(WK_POS + 4), f(WK_POS + 8)],
        behaviour,
        mode: u16::from_be_bytes([wk[(b + BHV_MODE) as usize], wk[(b + BHV_MODE + 1) as usize]]),
        timer: be32(wk, b + BHV_TIMER) as i32,
    }
}

/// Read one Chao task's live state.
pub fn read_chao(w: &mut Worker, task: u32) -> io::Result<LiveChao> {
    let wk_addr = w.r32(task + TASK_TWP)?;
    let wk = w.read(wk_addr, WK_BHV + BHV_LIST)?;
    let idx = be32(&wk, WK_BHV + BHV_INDEX);
    let behaviour = w.r32(wk_addr + WK_BHV + BHV_LIST + 4 * idx)?;
    Ok(decode(task, &wk, behaviour))
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
        let c = decode(0x8100_0000, &wk, 0x8050_0000);
        assert_eq!(c.pos, [-58.0, 2.0, 5.0]);
        assert_eq!((c.mode, c.timer, c.behaviour), (3, 85, 0x8050_0000));
    }
}
