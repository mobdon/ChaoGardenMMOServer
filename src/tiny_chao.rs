//! A player's own Chao model for Tiny Chao Garden on their phone.
//!
//! The PC game page makes it from the player's own game files and stores it here the
//! first time they play. The phone, logged in to the same account, then reads it back.
//! It is the only game-derived file the server keeps; until it exists the phone's Tiny
//! Garden does not connect.

use std::path::PathBuf;

/// Largest Chao model a PC may store for its phone.
pub const MODEL_MAX: usize = 4 << 20;

/// The stored model's media type by its magic bytes: a PNG picture or a binary glTF model.
pub fn model_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"glTF") {
        Some("model/gltf-binary")
    } else {
        None
    }
}
/// Where a player's own Chao model for Tiny Garden is kept. It is the only game-derived
/// file the server stores, made on the player's PC from their own game files.
pub fn model_path(data_dir: &std::path::Path, chao_id: &str) -> PathBuf {
    // Hashed so any stored id is a safe file name.
    data_dir
        .join("tiny-chao")
        .join(format!("{}.bin", crate::game::hash_token(chao_id)))
}

/// True once the player has played on their PC and their Chao model is stored.
pub fn played_once(data_dir: &std::path::Path, chao_id: &str) -> bool {
    model_path(data_dir, chao_id).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_type_goes_by_magic_bytes() {
        assert_eq!(model_type(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(model_type(b"glTF\x02\0\0\0"), Some("model/gltf-binary"));
        assert_eq!(model_type(b"<html>"), None);
        assert_eq!(model_type(b""), None);
    }

    #[test]
    fn model_path_is_a_hashed_name_inside_the_data_dir() {
        let path = model_path(std::path::Path::new("/data"), "../../etc/passwd");
        assert!(path.starts_with("/data/tiny-chao"));
        assert_eq!(path.components().count(), 4);
    }
}
