//! Private verifier harness: plays using only the public initial peek and pushes.
//! No network access and no live save path.
use std::{
    collections::VecDeque,
    env, fs,
    io::{self, Read, Write},
};
use tiny_garden_core::native_memory::*;

const FRAME_LIMIT: usize = 5000;

fn phase(value: NativeMemoryPhase) -> u8 {
    match value {
        NativeMemoryPhase::Reveal => 4,
        NativeMemoryPhase::Shuffle => 5,
        NativeMemoryPhase::Matching => 6,
        NativeMemoryPhase::Result => 7,
        NativeMemoryPhase::Ended => 0,
    }
}

fn record(out: &mut Vec<u8>, input: NativeButtons, view: &NativeMemoryView) {
    out.extend_from_slice(&input.pressed.to_le_bytes());
    out.extend_from_slice(&input.held.to_le_bytes());
    out.extend_from_slice(&[
        phase(view.phase),
        view.mistakes,
        view.matched_pairs,
        view.rings,
    ]);
    out.extend_from_slice(&view.cursor.map_or([255, 255], |(x, y)| [x, y]));
    for cell in &view.cells {
        out.extend_from_slice(&[u8::from(cell.occupied), cell.face.unwrap_or(255)]);
    }
    if let Some(walker) = view.walker {
        out.push(1);
        out.extend_from_slice(&walker.x.to_le_bytes());
        out.extend_from_slice(&walker.y.to_le_bytes());
        out.extend_from_slice(&[walker.direction as u8, walker.trip]);
        if let Some((x, y)) = walker.moving_position {
            out.push(1);
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
        } else {
            out.extend_from_slice(&[0; 5]);
        }
    } else {
        out.extend_from_slice(&[0; 12]);
    }
}

fn main() {
    let path = env::args_os()
        .nth(1)
        .expect("verified private payload path");
    let payload = fs::read(path).unwrap();
    let source = A2neMemorySource::from_verified_payload(&payload).unwrap();
    let mut input = Vec::new();
    io::stdin().read_to_end(&mut input).unwrap();
    assert_eq!(input.len() % 5, 0);
    let mut output = Vec::new();
    for case in input.as_chunks::<5>().0 {
        let seed = u32::from_le_bytes(case[..4].try_into().unwrap());
        let mode = case[4];
        assert!(mode <= 4);
        let mistakes_target = match mode {
            2 => 3,
            3 => 1,
            4 => 2,
            _ => 0,
        };
        let mut round = source.begin_round(seed).unwrap();
        let mut previous = round.view();
        let mut knowledge = previous.cells.map(|cell| cell.face);
        let mut selections = VecDeque::new();
        let mut trace = Vec::new();
        let mut tripped = false;
        for _ in 0..FRAME_LIMIT {
            let view = round.view();
            let mut buttons = NativeButtons::default();
            match view.phase {
                NativeMemoryPhase::Shuffle => {
                    if mode != 0
                        && !tripped
                        && view
                            .walker
                            .is_some_and(|walker| walker.direction >= 2 && walker.trip == 0)
                    {
                        buttons.pressed = 1;
                        tripped = true;
                    }
                }
                NativeMemoryPhase::Matching => {
                    if selections.is_empty() && view.selected.iter().all(Option::is_none) {
                        if view.mistakes < mistakes_target {
                            let first = (0..CELLS)
                                .find(|&index| view.cells[index].occupied)
                                .unwrap();
                            let second = (0..CELLS)
                                .find(|&index| {
                                    view.cells[index].occupied
                                        && knowledge[index] != knowledge[first]
                                })
                                .unwrap();
                            selections.extend([first, second]);
                        } else {
                            let first = (0..CELLS)
                                .find(|&index| view.cells[index].occupied)
                                .unwrap();
                            let second = (0..CELLS)
                                .find(|&index| {
                                    index != first
                                        && view.cells[index].occupied
                                        && knowledge[index] == knowledge[first]
                                })
                                .unwrap();
                            selections.extend([first, second]);
                        }
                    }
                    if let Some(&cell) = selections.front() {
                        let (column, row) = ((cell % COLUMNS) as u8, (cell / COLUMNS) as u8);
                        let cursor = view.cursor.unwrap();
                        buttons.pressed = if cursor.0 != column {
                            16
                        } else if cursor.1 != row {
                            128
                        } else {
                            selections.pop_front();
                            1
                        };
                    }
                }
                NativeMemoryPhase::Ended => break,
                _ => {}
            }
            round.step(buttons).unwrap();
            let current = round.view();
            let from = (0..CELLS)
                .find(|&cell| previous.cells[cell].occupied && !current.cells[cell].occupied);
            let to = (0..CELLS)
                .find(|&cell| !previous.cells[cell].occupied && current.cells[cell].occupied);
            if let (Some(from), Some(to)) = (from, to) {
                knowledge[to] = knowledge[from];
                knowledge[from] = None;
            }
            for (index, cell) in current.cells.iter().enumerate() {
                if let Some(face) = cell.face {
                    knowledge[index] = Some(face);
                }
            }
            record(&mut trace, buttons, &current);
            previous = current;
        }
        assert_eq!(round.view().phase, NativeMemoryPhase::Ended);
        assert_eq!(round.finished().unwrap().won, mode != 2);
        assert_eq!(trace.len() % 82, 0);
        output.extend_from_slice(&(trace.len() as u32 / 82).to_le_bytes());
        output.extend_from_slice(&trace);
    }
    io::stdout().write_all(&output).unwrap();
}
