import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import backup
import preflight


class BackupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "data"
        self.source.mkdir()
        self.profiles = [{"id": "canonical", "tokenHash": "a" * 64, "rings": 88,
                          "tinyRecords": {"memory": {"lastResultId": "receipt-once"}},
                          "resident": {"enabled": True, "zone": "garden"}}]
        (self.source / "profiles.json").write_text(json.dumps(self.profiles))
        (self.source / "profiles.json.tmp").write_text("uncommitted")
        (self.source / "terrain").mkdir()
        (self.source / "terrain/garden.json").write_text('{"heights":[0]}')
        self.archive = self.root / "backup.zip"

    def test_roundtrip_preserves_identity_receipt_and_bytes(self):
        manifest = backup.create(self.source, self.archive)
        self.assertEqual(set(manifest["files"]), {"profiles.json", "terrain/garden.json"})
        destination = self.root / "restored"
        backup.restore(self.archive, destination)
        self.assertEqual((self.source / "profiles.json").read_bytes(),
                         (destination / "profiles.json").read_bytes())
        self.assertFalse((destination / "profiles.json.tmp").exists())
        with self.assertRaises(ValueError):
            backup.restore(self.archive, destination)
        with self.assertRaises(FileExistsError):
            backup.create(self.source, self.archive)

    def test_corruption_and_unsafe_paths_rejected(self):
        backup.create(self.source, self.archive)
        changed = self.root / "changed.zip"
        with zipfile.ZipFile(self.archive) as original, zipfile.ZipFile(changed, "w") as modified:
            for name in original.namelist():
                modified.writestr(name, b"[]" if name == "profiles.json" else original.read(name))
        with self.assertRaises(ValueError):
            backup.restore(changed, self.root / "bad")
        self.assertFalse((self.root / "bad").exists())
        for name in ("../profile", "/profile", "C:/profile", "a\\b", "a/./b", "a//b"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                backup.safe_name(name)

    def test_duplicate_identity_refused_and_partial_removed(self):
        (self.source / "profiles.json").write_text(json.dumps(self.profiles * 2))
        with self.assertRaises(ValueError):
            backup.create(self.source, self.archive)
        self.assertFalse(self.archive.exists())

    def test_missing_identity_or_invalid_store_refused(self):
        for profiles in ({}, [None], [{"id": ""}],
                         [{"id": "canonical", "tokenHash": "plaintext"}]):
            with self.subTest(profiles=profiles):
                (self.source / "profiles.json").write_text(json.dumps(profiles))
                with self.assertRaises(ValueError):
                    backup.create(self.source, self.archive)
                self.assertFalse(self.archive.exists())
        (self.source / "profiles.json").write_text(json.dumps(self.profiles))
        with patch("backup.os.scandir", side_effect=PermissionError("unreadable source")):
            with self.assertRaises(PermissionError):
                backup.create(self.source, self.archive)
        self.assertFalse(self.archive.exists())

    def test_private_preflight(self):
        environment = {"CHAO_DATA_DIR": str(self.source), "GAME_HOST": "127.0.0.1"}
        preflight.check(environment)
        for changes in ({"GAME_HOST": "0.0.0.0"}, {"CHAO_OWNER_BOOTSTRAP": "true"},
                        {"CHAO_DATA_DIR": "relative"}, {"CHAO_SHUTDOWN_STDIN": "1"}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                preflight.check(environment | changes)


if __name__ == "__main__":
    unittest.main()
