//! Build `imports/chao-motion-table.json` from the owner's own GSNE8P
//! `ChaoMain.rel`. The output is game data: keep it in the local data
//! directory and never commit it.
//!
//! Usage: chao-motion-table <ChaoMain.rel> <output.json>
use chao_game_server::behaviour::MotionTable;
use sha2::{Digest, Sha256};
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let [_, input, output] = args.as_slice() else {
        eprintln!("usage: chao-motion-table <ChaoMain.rel> <output.json>");
        return ExitCode::from(2);
    };
    let run = || -> Result<String, String> {
        let rel = fs::read(input).map_err(|e| format!("{input}: {e}"))?;
        let table = MotionTable::from_rel(&rel)?;
        let json = serde_json::to_vec(table.rows()).map_err(|e| e.to_string())?;
        fs::write(output, json).map_err(|e| format!("{output}: {e}"))?;
        Ok(format!(
            "{} motion states written to {output} (ChaoMain.rel sha256 {:x})",
            table.rows().len(),
            Sha256::digest(&rel)
        ))
    };
    match run() {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
