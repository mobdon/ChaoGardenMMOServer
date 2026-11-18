use serde_json::json;
use std::{
    env,
    error::Error,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use tiny_garden_core::{gba, native_save};

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(format!("Input exceeds the {limit}-byte limit").into());
    }
    Ok(bytes)
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let command = args.first().and_then(|arg| arg.to_str()).unwrap_or("");
    if !matches!(
        command,
        "header" | "scan" | "save" | "verify-payload" | "extract-reference"
    ) || args.len() != if command == "extract-reference" { 3 } else { 2 }
    {
        return Err("Usage: tiny-garden-inspect <header|scan|save|verify-payload> RAW_FILE\n       tiny-garden-inspect extract-reference RAW_ROM NEW_OUTPUT_FILE".into());
    }
    let path = Path::new(&args[1]);
    let limit = if command == "save" {
        2 * 1024 * 1024
    } else if command == "verify-payload" {
        gba::EWRAM_LEN
    } else {
        gba::MAX_ROM_LEN
    };
    let bytes = read_bounded(path, limit)?;
    let report = match command {
        "header" => {
            json!({"scope":"Raw GBA header inspection; no ROM execution or entitlement proof", "bytes":bytes.len(),
            "header":gba::GbaHeader::parse(&bytes)?.report(), "sha1":gba::sha1(&bytes), "sha256":gba::sha256(&bytes)})
        }
        "scan" => {
            json!({"scope":"Search for one fingerprint-verified USA Tiny Chao Garden payload; other versions remain unaudited",
            "bytes":bytes.len(), "header":gba::GbaHeader::parse(&bytes)?.report(), "sha1":gba::sha1(&bytes),
            "knownPayload":gba::find_reference_garden(&bytes)?})
        }
        "save" => {
            let flash = native_save::inspect_flash(&bytes)?;
            json!({"scope":"Read-only native sector validation; every body field remains opaque", "bytes":bytes.len(),
                "sectors":flash.sectors,"selectedPhysicalSector":flash.selected_physical_sector,
                "selectedRecordSha256":flash.selected.as_ref().map(|record| gba::sha256(record.raw_bytes())),
                "opaqueBodyBytes":flash.selected.as_ref().map(|record| record.opaque_body().len()),
                "trailingBytesAfterFirstBank":flash.trailing_bytes_after_first_bank})
        }
        "verify-payload" => {
            if gba::sha1(&bytes) != gba::REFERENCE_GARDEN_SHA1 {
                return Err(
                    "Unknown payload fingerprint: USA table offset cannot be assumed".into(),
                );
            }
            native_save::verify_reference_crc_table(&bytes)?;
            json!({"verifiedPayloadSha1":gba::sha1(&bytes),"crcTableEntriesVerified":256,"reflectedPolynomial":"0x8408",
                "initial":"0xffff","finalXor":"0xffff"})
        }
        "extract-reference" => {
            let decoded = gba::extract_reference_garden(&bytes)?;
            native_save::verify_reference_crc_table(&decoded.bytes)?;
            // Extraction is explicit and never overwrites an existing file.
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(Path::new(&args[2]))?;
            output.write_all(&decoded.bytes)?;
            json!({"scope":"Private local decoded program, not an engine port or a redistributable web asset",
                "romOffset":gba::REFERENCE_GARDEN_ROM_OFFSET,"compressedBytes":decoded.consumed,"decodedBytes":decoded.bytes.len(),
                "sha1":gba::sha1(&decoded.bytes),"sha256":gba::sha256(&decoded.bytes),"crcTableEntriesVerified":256})
        }
        _ => unreachable!(),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
