//! Local, source-checked retail item rows. No decoded retail table is distributed.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fmt, fs, io::Read, path::Path};

pub const CATALOG_RULES_VERSION: &str = "sa2-steam-audited-item-rows-v1";
pub const RAW_UNITS: &str = "signed-native-item-table-units";
pub const AUDITED_EXECUTABLE_SHA256: &str =
    "4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16";
const STATS_ADDRESS: u32 = 0x008a6240;
const FRUIT_STATS_ADDRESS: u32 = 0x008a6448;
pub const FRUIT_COUNT: u8 = 24;
/// Native fruit enum order 0x00-0x17 (client docs/MECHANICS.md, CAT-FOOD-01).
pub const FRUIT_KEYS: [&str; 24] = [
    "chao-garden",
    "hero-garden",
    "dark-garden",
    "strong",
    "tasty",
    "hero",
    "dark",
    "round",
    "triangle",
    "square",
    "heart",
    "chao",
    "smart",
    "orange",
    "blue",
    "pink",
    "green",
    "purple",
    "yellow",
    "red",
    "mushroom",
    "mushroom-alt",
    "mint-candy",
    "grapes",
];
const FRUIT_NAMES: [&str; 24] = [
    "Chao Garden Fruit",
    "Hero Garden Fruit",
    "Dark Garden Fruit",
    "Strong Fruit",
    "Tasty Fruit",
    "Hero Fruit",
    "Dark Fruit",
    "Round Fruit",
    "Triangle Fruit",
    "Square Fruit",
    "Heart Fruit",
    "Chao Fruit",
    "Smart Fruit",
    "Orange Fruit",
    "Blue Fruit",
    "Pink Fruit",
    "Green Fruit",
    "Purple Fruit",
    "Yellow Fruit",
    "Red Fruit",
    "Mushroom",
    "Alternate Mushroom",
    "Mint Candy",
    "Grapes",
];
/// The eight persistent native `MinimalParts` categories. Each expands to the
/// thirteen render sockets named by the client audit.
pub const PART_CATEGORIES: [&str; 8] = [
    "arms", "ears", "forehead", "horns", "legs", "tail", "wings", "tongue",
];
fn part_category(slot: &str) -> Option<usize> {
    let category = match slot {
        "leftArm" | "rightArm" => "arms",
        "leftEar" | "rightEar" => "ears",
        "forehead" => "forehead",
        "leftHorn" | "rightHorn" => "horns",
        "leftLeg" | "rightLeg" => "legs",
        "tail" => "tail",
        "leftWing" | "rightWing" => "wings",
        "tongue" => "tongue",
        _ => return None,
    };
    PART_CATEGORIES.iter().position(|c| *c == category)
}
const MAX_AUDIT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_EXECUTABLE_BYTES: u64 = 64 * 1024 * 1024;
const KEYS: [&str; 25] = [
    "penguin",
    "seal",
    "otter",
    "rabbit",
    "cheetah",
    "warthog",
    "bear",
    "tiger",
    "gorilla",
    "peacock",
    "parrot",
    "condor",
    "skunk",
    "sheep",
    "raccoon",
    "half-fish",
    "skeleton-dog",
    "bat",
    "dragon",
    "unicorn",
    "phoenix",
    "yellow-chaos-drive",
    "green-chaos-drive",
    "red-chaos-drive",
    "purple-chaos-drive",
];

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NativeItemStats {
    pub mood: i16,
    pub belly: i16,
    pub swim: i16,
    pub fly: i16,
    pub run: i16,
    pub power: i16,
    pub stamina: i16,
    pub luck: i16,
    pub intelligence: i16,
    #[serde(rename = "unknown18")]
    pub unknown_18: i16,
}

impl NativeItemStats {
    pub fn skill_deltas(&self) -> [(&'static str, i16); 7] {
        [
            ("swim", self.swim),
            ("fly", self.fly),
            ("run", self.run),
            ("power", self.power),
            ("stamina", self.stamina),
            ("luck", self.luck),
            ("intelligence", self.intelligence),
        ]
    }

    fn fields(self) -> [i16; 10] {
        [
            self.mood,
            self.belly,
            self.swim,
            self.fly,
            self.run,
            self.power,
            self.stamina,
            self.luck,
            self.intelligence,
            self.unknown_18,
        ]
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NativeItemKind {
    Animal,
    Drive,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeItem {
    pub id: u8,
    pub key: String,
    pub name: String,
    pub kind: NativeItemKind,
    pub raw: NativeItemStats,
    /// Part categories whose child and adult sockets all resolve to verified
    /// native models. Unresolved, runtime-only or partial categories are left
    /// out so the server never grants a part the client cannot render natively.
    pub part_categories: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeFruit {
    pub id: u8,
    pub key: &'static str,
    pub name: &'static str,
    pub raw: NativeItemStats,
}

/// Empty by default: unconfigured/invalid local evidence must never mint guessed effects.
#[derive(Clone, Debug, Default)]
pub struct CareCatalog {
    items: BTreeMap<u8, NativeItem>,
    fruits: BTreeMap<u8, NativeFruit>,
    source_sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogError(String);

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for CatalogError {}
fn invalid(message: &str) -> CatalogError {
    CatalogError(message.into())
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, CatalogError> {
    let metadata = fs::metadata(path).map_err(|_| invalid("Care source file is unavailable"))?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(invalid(
            "Care source file exceeds its bound or is not a regular file",
        ));
    }
    let file = fs::File::open(path).map_err(|_| invalid("Care source file could not be read"))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("Care source file could not be read"))?;
    if bytes.len() as u64 > limit {
        return Err(invalid("Care source file exceeds its bound"));
    }
    Ok(bytes)
}

impl CareCatalog {
    /// Made-up rows for transaction unit tests only; these are never retail data.
    /// ID 0 (and other animals) uses [mood -3, belly 5, seven skills
    /// 11/-13/17/-19/23/-29/31, unknown -37]. Drives use +11 in their
    /// principal channel, -3 Luck and +7 Intelligence. No source hash is claimed.
    #[cfg(test)]
    pub fn synthetic() -> Self {
        let items = KEYS
            .iter()
            .enumerate()
            .map(|(index, key)| {
                let raw = if index < 21 {
                    NativeItemStats {
                        mood: -3,
                        belly: 5,
                        swim: 11,
                        fly: -13,
                        run: 17,
                        power: -19,
                        stamina: 23,
                        luck: -29,
                        intelligence: 31,
                        unknown_18: -37,
                    }
                } else {
                    let mut row = NativeItemStats {
                        luck: -3,
                        intelligence: 7,
                        ..Default::default()
                    };
                    match index {
                        21 => row.swim = 11,
                        22 => row.run = 11,
                        23 => row.power = 11,
                        24 => row.fly = 11,
                        _ => unreachable!(),
                    }
                    row
                };
                let id = index as u8;
                (
                    id,
                    NativeItem {
                        id,
                        key: (*key).into(),
                        name: format!("Synthetic fixture {index}"),
                        kind: if index < 21 {
                            NativeItemKind::Animal
                        } else {
                            NativeItemKind::Drive
                        },
                        raw,
                        // Synthetic: every animal provides ears and tail; Rabbit (3)
                        // has none, mirroring the unresolved-ear fail-closed path.
                        part_categories: if index < 21 && index != 3 {
                            vec!["ears", "tail"]
                        } else {
                            vec![]
                        },
                    },
                )
            })
            .collect();
        // Made-up fruit rows: mood +2, belly +40, stamina +id, intelligence -1.
        let fruits = (0..FRUIT_COUNT)
            .map(|id| {
                (
                    id,
                    NativeFruit {
                        id,
                        key: FRUIT_KEYS[id as usize],
                        name: FRUIT_NAMES[id as usize],
                        raw: NativeItemStats {
                            mood: 2,
                            belly: 40,
                            stamina: i16::from(id),
                            intelligence: -1,
                            ..Default::default()
                        },
                    },
                )
            })
            .collect();
        Self {
            items,
            fruits,
            source_sha256: None,
        }
    }

    /// `audit_dir` is the client extractor's ignored `.local/care` directory.
    /// The sibling `.local/extracted/care/manifest.json` supplies source provenance.
    /// The executable is read-only; no game or profile is modified.
    pub fn load(audit_dir: &Path, executable: &Path) -> Result<Self, CatalogError> {
        let root = audit_dir
            .parent()
            .ok_or_else(|| invalid("Care audit directory has no parent"))?;
        let audit = read_bounded(&audit_dir.join("animals.json"), MAX_AUDIT_BYTES)?;
        let report = read_bounded(&audit_dir.join("verification-report.json"), MAX_AUDIT_BYTES)?;
        let manifest = read_bounded(&root.join("extracted/care/manifest.json"), MAX_AUDIT_BYTES)?;
        let binary = read_bounded(executable, MAX_EXECUTABLE_BYTES)?;
        Self::from_verified_bytes(&audit, &report, &manifest, &binary)
    }

    /// The trusted server operator supplies local evidence, never an incoming client.
    pub fn from_verified_bytes(
        audit: &[u8],
        report: &[u8],
        manifest: &[u8],
        executable: &[u8],
    ) -> Result<Self, CatalogError> {
        if [audit.len(), report.len(), manifest.len()]
            .into_iter()
            .any(|n| n as u64 > MAX_AUDIT_BYTES)
            || executable.len() as u64 > MAX_EXECUTABLE_BYTES
        {
            return Err(invalid("Care evidence exceeds its bound"));
        }
        let audit: Value =
            serde_json::from_slice(audit).map_err(|_| invalid("Invalid care audit JSON"))?;
        let report: Value = serde_json::from_slice(report)
            .map_err(|_| invalid("Invalid care verification JSON"))?;
        let manifest: Value = serde_json::from_slice(manifest)
            .map_err(|_| invalid("Invalid care provenance JSON"))?;
        if audit["version"] != 1
            || report["version"] != 1
            || manifest["version"] != 1
            || report["passed"] != true
            || report["animals"] != 21
            || report["drives"] != 4
            || report["nativeStatRows"] != 26
            || report["fruitStatRows"] != 24
            || report["independentSAtoolsStatRowsMatched"] != 50
            || report["originalsUnchanged"] != 25
        {
            return Err(invalid(
                "Care audit is not the supported independently verified version",
            ));
        }
        let digest = format!("{:x}", Sha256::digest(executable));
        if digest != AUDITED_EXECUTABLE_SHA256 {
            return Err(invalid(
                "Executable is not the audited Steam source revision",
            ));
        }
        let sources = manifest["sources"]
            .as_array()
            .ok_or_else(|| invalid("Missing care source provenance"))?;
        let matches: Vec<_> = sources
            .iter()
            .filter(|s| s["relativePath"] == "sonic2app.exe")
            .collect();
        if sources.len() != 25
            || matches.len() != 1
            || matches[0]["sha256"] != digest
            || matches[0]["bytes"] != executable.len() as u64
            || sources.iter().any(|s| s["unchanged"] != true)
        {
            return Err(invalid("Care provenance does not match the audited source"));
        }
        let animals = audit["animals"]
            .as_array()
            .ok_or_else(|| invalid("Missing native animals"))?;
        let drives = audit["drives"]
            .as_array()
            .ok_or_else(|| invalid("Missing native drives"))?;
        if animals.len() != 21 || drives.len() != 4 {
            return Err(invalid(
                "Native care catalog must contain 21 animals and four drives",
            ));
        }
        let mut items = BTreeMap::new();
        for (index, row) in animals.iter().chain(drives.iter()).enumerate() {
            let id = row["id"]
                .as_u64()
                .ok_or_else(|| invalid("Missing native item ID"))?;
            if id != index as u64 || row["key"] != KEYS[index] {
                return Err(invalid(
                    "Native care IDs or identities do not match the audited enum",
                ));
            }
            let name = row["name"]
                .as_str()
                .filter(|v| !v.is_empty() && v.len() <= 80)
                .ok_or_else(|| invalid("Invalid native item display name"))?;
            let raw: NativeItemStats = serde_json::from_value(row["nativeItemStats"].clone())
                .map_err(|_| invalid("Native item row must contain ten signed 16-bit fields"))?;
            let expected_address = STATS_ADDRESS + index as u32 * 20;
            if row["source"]["binary"] != "sonic2app.exe"
                || row["source"]["statsAddress"].as_str().and_then(parse_hex)
                    != Some(expected_address)
                || decode_fields(executable, expected_address)? != raw.fields()
            {
                return Err(invalid(
                    "Native item row differs from original executable bytes",
                ));
            }
            if index < 21 {
                if row["nativeHistoryBitHex"].as_str().and_then(parse_hex) != Some(1u32 << index) {
                    return Err(invalid("Animal species history identity is invalid"));
                }
            } else if row["grantsAnimalHistoryBit"] != false || row["animalParts"] != false {
                return Err(invalid(
                    "Chaos Drives must not grant species history or animal parts",
                ));
            }
            let part_categories = if index < 21 {
                verified_part_categories(&row["parts"])?
            } else {
                vec![]
            };
            items.insert(
                id as u8,
                NativeItem {
                    id: id as u8,
                    key: KEYS[index].into(),
                    name: name.into(),
                    kind: if index < 21 {
                        NativeItemKind::Animal
                    } else {
                        NativeItemKind::Drive
                    },
                    raw,
                    part_categories,
                },
            );
        }
        let fruit_rows = audit["fruitStats"]
            .as_array()
            .filter(|rows| rows.len() == usize::from(FRUIT_COUNT))
            .ok_or_else(|| invalid("Native fruit table must contain 24 rows"))?;
        let mut fruits = BTreeMap::new();
        for (index, row) in fruit_rows.iter().enumerate() {
            let raw: NativeItemStats = serde_json::from_value(row["values"].clone())
                .map_err(|_| invalid("Native fruit row must contain ten signed 16-bit fields"))?;
            let expected_address = FRUIT_STATS_ADDRESS + index as u32 * 20;
            if row["index"].as_u64() != Some(index as u64)
                || row["address"].as_str().and_then(parse_hex) != Some(expected_address)
                || decode_fields(executable, expected_address)? != raw.fields()
            {
                return Err(invalid(
                    "Native fruit row differs from original executable bytes",
                ));
            }
            fruits.insert(
                index as u8,
                NativeFruit {
                    id: index as u8,
                    key: FRUIT_KEYS[index],
                    name: FRUIT_NAMES[index],
                    raw,
                },
            );
        }
        Ok(Self {
            items,
            fruits,
            source_sha256: Some(digest),
        })
    }

    pub fn fruit(&self, id: u8) -> Option<&NativeFruit> {
        self.fruits.get(&id)
    }

    pub fn item(&self, id: u8) -> Option<&NativeItem> {
        self.items.get(&id)
    }
    pub fn items(&self) -> impl Iterator<Item = &NativeItem> {
        self.items.values()
    }
    pub fn enabled(&self) -> bool {
        self.items.len() == 25
    }
    pub fn fruits_enabled(&self) -> bool {
        self.fruits.len() == usize::from(FRUIT_COUNT)
    }
    pub fn source_sha256(&self) -> Option<&str> {
        self.source_sha256.as_deref()
    }
    pub fn public(&self) -> Value {
        json!({"enabled":self.enabled(),"rulesVersion":CATALOG_RULES_VERSION,
            "classification":"verified-native-data","units":RAW_UNITS,
            "sourceSha256":self.source_sha256,"items":self.items.values().collect::<Vec<_>>(),
            "nativeApplicationFormula":"unknown","nativeBehaviorAcquisition":"unknown",
            "nativeEvolutionCriteria":"unknown","reservedRow25Enabled":false,
            "fruits":self.fruits.values().collect::<Vec<_>>(),"fruitsEnabled":self.fruits_enabled(),
            "fruitSpecialEffects":"unknown-not-applied","partCategories":PART_CATEGORIES,
            "partSelection":"adapted-replace-every-verified-category-v1"})
    }
}

/// A category is grantable only when both the child and adult native lists are
/// file-backed and every socket of that category resolves to exactly one model.
fn verified_part_categories(parts: &Value) -> Result<Vec<&'static str>, CatalogError> {
    let mut usable = [true; 8];
    let mut present = [false; 8];
    for age in ["child", "adult"] {
        let list = &parts[age];
        let status = list["status"]
            .as_str()
            .ok_or_else(|| invalid("Missing native part list status"))?;
        let slots = list["slots"]
            .as_array()
            .ok_or_else(|| invalid("Missing native part slots"))?;
        if status == "absent" {
            continue;
        }
        if slots.len() != 13 {
            // Runtime-only or incomplete lists grant nothing until resolved.
            usable = [false; 8];
            continue;
        }
        for slot in slots {
            let name = slot["slot"]
                .as_str()
                .ok_or_else(|| invalid("Missing native part slot name"))?;
            let category =
                part_category(name).ok_or_else(|| invalid("Unknown native part slot"))?;
            if slot["pointer"].is_null() {
                continue;
            }
            present[category] = true;
            if slot["mapped"] != true || !slot["model"].is_object() {
                usable[category] = false;
            }
        }
    }
    Ok(PART_CATEGORIES
        .iter()
        .enumerate()
        .filter(|(i, _)| present[*i] && usable[*i])
        .map(|(_, c)| *c)
        .collect())
}

fn parse_hex(value: &str) -> Option<u32> {
    u32::from_str_radix(value.strip_prefix("0x")?, 16).ok()
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, CatalogError> {
    let data = bytes
        .get(
            offset
                ..offset
                    .checked_add(2)
                    .ok_or_else(|| invalid("PE offset overflow"))?,
        )
        .ok_or_else(|| invalid("Truncated executable"))?;
    Ok(u16::from_le_bytes([data[0], data[1]]))
}
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, CatalogError> {
    let data = bytes
        .get(
            offset
                ..offset
                    .checked_add(4)
                    .ok_or_else(|| invalid("PE offset overflow"))?,
        )
        .ok_or_else(|| invalid("Truncated executable"))?;
    Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Decode only a bounded, file-backed PE32 section; virtual-only pointers are rejected.
fn decode_fields(binary: &[u8], address: u32) -> Result<[i16; 10], CatalogError> {
    if binary.get(..2) != Some(b"MZ") {
        return Err(invalid("Missing executable DOS signature"));
    }
    let pe = read_u32(binary, 60)? as usize;
    if binary.get(
        pe..pe
            .checked_add(4)
            .ok_or_else(|| invalid("PE offset overflow"))?,
    ) != Some(b"PE\0\0")
    {
        return Err(invalid("Missing executable PE signature"));
    }
    let sections = read_u16(binary, pe + 6)? as usize;
    let optional_size = read_u16(binary, pe + 20)? as usize;
    let optional = pe
        .checked_add(24)
        .ok_or_else(|| invalid("PE offset overflow"))?;
    if sections == 0 || sections > 96 || optional_size < 32 || read_u16(binary, optional)? != 0x10b
    {
        return Err(invalid("Unsupported executable section layout"));
    }
    let image_base = read_u32(binary, optional + 28)?;
    let rva = address
        .checked_sub(image_base)
        .ok_or_else(|| invalid("Native row precedes image base"))?;
    let table = optional
        .checked_add(optional_size)
        .ok_or_else(|| invalid("PE offset overflow"))?;
    let mut match_offset = None;
    for index in 0..sections {
        let at = table
            .checked_add(index * 40)
            .ok_or_else(|| invalid("PE offset overflow"))?;
        let start = read_u32(binary, at + 12)?;
        let size = read_u32(binary, at + 16)?;
        let Some(relative) = rva.checked_sub(start) else {
            continue;
        };
        if relative.checked_add(20).is_none_or(|end| end > size) {
            continue;
        }
        let raw = read_u32(binary, at + 20)?;
        let offset = raw
            .checked_add(relative)
            .ok_or_else(|| invalid("PE row offset overflow"))? as usize;
        if match_offset.replace(offset).is_some() {
            return Err(invalid("Ambiguous native row sections"));
        }
    }
    let offset = match_offset.ok_or_else(|| invalid("Native row is not file-backed"))?;
    let mut result = [0i16; 10];
    for (index, value) in result.iter_mut().enumerate() {
        *value = read_u16(binary, offset + index * 2)? as i16;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic PE and rows, deliberately unrelated to copyrighted retail table values.
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0u8; 512];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[70..72].copy_from_slice(&1u16.to_le_bytes());
        bytes[84..86].copy_from_slice(&32u16.to_le_bytes());
        bytes[88..90].copy_from_slice(&0x10bu16.to_le_bytes());
        bytes[116..120].copy_from_slice(&0x00400000u32.to_le_bytes());
        bytes[132..136].copy_from_slice(&0x004a6240u32.to_le_bytes());
        bytes[136..140].copy_from_slice(&20u32.to_le_bytes());
        bytes[140..144].copy_from_slice(&256u32.to_le_bytes());
        for (index, value) in [-3i16, 5, -7, 11, -13, 17, -19, 23, -29, 31]
            .into_iter()
            .enumerate()
        {
            bytes[256 + index * 2..258 + index * 2].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn signed_native_row_decoding_keeps_every_field() {
        assert_eq!(
            decode_fields(&fixture(), STATS_ADDRESS).unwrap(),
            [-3, 5, -7, 11, -13, 17, -19, 23, -29, 31]
        );
    }
    #[test]
    fn rejects_truncated_and_unbacked_rows() {
        for length in 0..276 {
            assert!(decode_fields(&fixture()[..length], STATS_ADDRESS).is_err());
        }
        assert!(decode_fields(&fixture(), STATS_ADDRESS + 1).is_err());
        assert!(decode_fields(&fixture(), 0x003fffff).is_err());
        let mut overflowing = fixture();
        overflowing[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_fields(&overflowing, STATS_ADDRESS).is_err());
    }
    #[test]
    fn unsupported_or_missing_evidence_fails_closed() {
        let catalog = CareCatalog::default();
        assert!(!catalog.enabled());
        assert!(catalog.item(0).is_none());
        assert!(catalog.item(25).is_none());
        assert!(CareCatalog::from_verified_bytes(b"{}", b"{}", b"{}", &fixture()).is_err());
        assert_eq!(catalog.public()["nativeApplicationFormula"], "unknown");
    }
    fn slots(missing: &[&str], unmapped: &[&str]) -> Value {
        let names = [
            "leftArm",
            "rightArm",
            "leftEar",
            "rightEar",
            "forehead",
            "leftHorn",
            "rightHorn",
            "leftLeg",
            "rightLeg",
            "tail",
            "leftWing",
            "rightWing",
            "tongue",
        ];
        Value::Array(
            names
                .iter()
                .map(|n| {
                    if missing.contains(n) {
                        json!({"slot":n,"pointer":null,"model":null,"mapped":true})
                    } else if unmapped.contains(n) {
                        json!({"slot":n,"pointer":"0x1","model":null,"mapped":false})
                    } else {
                        json!({"slot":n,"pointer":"0x1","model":{"name":"m"},"mapped":true})
                    }
                })
                .collect(),
        )
    }

    #[test]
    fn part_categories_need_every_socket_resolved_in_child_and_adult_lists() {
        let none = [
            "leftArm",
            "rightArm",
            "forehead",
            "leftHorn",
            "rightHorn",
            "leftLeg",
            "rightLeg",
            "leftWing",
            "rightWing",
            "tongue",
        ];
        let ok = json!({"status":"file-backed-models-mapped","slots":slots(&none,&[])});
        let parts = json!({"child":ok,"adult":ok});
        assert_eq!(
            verified_part_categories(&parts).unwrap(),
            vec!["ears", "tail"]
        );
        // One unresolved ear (the Rabbit case) removes the whole ears category.
        let broken = json!({"status":"file-backed-model-mappings-incomplete","slots":slots(&none,&["rightEar"])});
        let parts = json!({"child":broken,"adult":ok});
        assert_eq!(verified_part_categories(&parts).unwrap(), vec!["tail"]);
        // Runtime-only lists (Half Fish, Skeleton Dog, Bat) grant nothing.
        let runtime = json!({"status":"runtime-list-not-file-backed","slots":[]});
        let parts = json!({"child":runtime,"adult":ok});
        assert!(verified_part_categories(&parts).unwrap().is_empty());
        assert!(verified_part_categories(&json!({})).is_err());
    }

    #[test]
    fn signed_stats_reject_missing_unsigned_overflow_and_claimed_fields() {
        let raw = NativeItemStats {
            power: -13,
            luck: -29,
            ..Default::default()
        };
        let mut json = serde_json::to_value(raw).unwrap();
        assert_eq!(
            serde_json::from_value::<NativeItemStats>(json.clone()).unwrap(),
            raw
        );
        json["power"] = json!(32768);
        assert!(serde_json::from_value::<NativeItemStats>(json.clone()).is_err());
        json["power"] = json!(-13);
        json["grade"] = json!(5);
        assert!(serde_json::from_value::<NativeItemStats>(json).is_err());
        assert!(serde_json::from_value::<NativeItemStats>(json!({"power":-13})).is_err());
        assert_eq!(raw.skill_deltas().len(), 7);
    }
}
