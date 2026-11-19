"""Offline, checksummed JSON-store backups. No service/network control.

Stop the only authority first. The --offline flag records that operator action;
it cannot detect another process writing the same directory. backup.sh provides
the systemd stop/start boundary. Restore always creates a NEW directory.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import tempfile
import zipfile

MANIFEST = "backup-manifest.json"
MAX_FILES = 100_000
MAX_BYTES = 64 * 1024**3


def digest(data):
    return hashlib.sha256(data).hexdigest()


def stream_digest(stream):
    size, hashed = 0, hashlib.sha256()
    while chunk := stream.read(1024 * 1024):
        size += len(chunk)
        hashed.update(chunk)
    return {"bytes": size, "sha256": hashed.hexdigest()}


def sync_directory(path):
    if os.name == "posix":
        descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def fail_walk(error):
    # os.walk otherwise skips unreadable subdirectories and can silently produce
    # a valid-looking but incomplete "full data" backup.
    raise error


def validate_profiles(data):
    profiles = json.loads(data)
    if not isinstance(profiles, list):
        raise ValueError("profiles.json must be an array")
    ids, credentials = set(), set()
    for profile in profiles:
        if not isinstance(profile, dict):
            raise ValueError("Invalid profile")
        identifier, credential = profile.get("id"), profile.get("tokenHash")
        if not isinstance(identifier, str) or not identifier or identifier in ids:
            raise ValueError("Missing/duplicate canonical profile id")
        if not isinstance(credential, str) or not re.fullmatch(r"[0-9a-f]{64}", credential):
            raise ValueError("Invalid hashed profile credential")
        if credential in credentials:
            raise ValueError("Duplicate profile credential")
        ids.add(identifier)
        credentials.add(credential)
    return len(profiles)


def safe_name(name):
    path = PurePosixPath(name)
    if (not name or path.is_absolute() or "\\" in name or ":" in name
            or any(part in (".", "..", "") for part in name.split("/"))):
        raise ValueError("Unsafe archive path")
    return path


def verify(archive):
    with zipfile.ZipFile(archive) as backup:
        members = backup.infolist()
        names = [member.filename for member in members]
        if len(names) != len(set(names)) or len(names) > MAX_FILES:
            raise ValueError("Duplicate/excess archive entries")
        if sum(member.file_size for member in members) > MAX_BYTES:
            raise ValueError("Backup exceeds the 64 GiB validation limit")
        for member in members:
            safe_name(member.filename)
            if member.is_dir() or stat.S_ISLNK(member.external_attr >> 16):
                raise ValueError("Directory/link archive entries are forbidden")
        manifest = json.loads(backup.read(MANIFEST))
        files = manifest.get("files")
        if manifest.get("format") != 1 or not isinstance(files, dict):
            raise ValueError("Unknown backup format")
        if set(files) != set(names) - {MANIFEST} or "profiles.json" not in files:
            raise ValueError("Manifest member mismatch or missing profiles")
        for name, expected in files.items():
            with backup.open(name) as member:
                actual = stream_digest(member)
            if expected != actual:
                raise ValueError("Checksum mismatch")
            if name == "profiles.json":
                validate_profiles(backup.read(name))
        return manifest


def create(source, archive):
    source, archive = Path(source).absolute(), Path(archive).absolute()
    if source.is_symlink() or not source.is_dir():
        raise ValueError("Source must be an ordinary directory")
    source = source.resolve()
    if archive.resolve().is_relative_to(source):
        raise ValueError("Backup must be outside its source directory")
    archive.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    files = {}
    created = False
    try:
        with archive.open("xb") as output:
            created = True
            os.chmod(archive, 0o600)
            with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_STORED) as backup:
                for root, directories, filenames in os.walk(source, followlinks=False, onerror=fail_walk):
                    if any((Path(root) / name).is_symlink() for name in directories):
                        raise ValueError("Source symlink directories are forbidden")
                    for name in sorted(filenames):
                        path = Path(root) / name
                        if path.is_symlink() or not path.is_file():
                            raise ValueError("Source links/special files are forbidden")
                        relative = path.relative_to(source).as_posix()
                        if relative == "profiles.json.tmp":
                            continue  # Uncommitted save data is never restored.
                        if relative == MANIFEST:
                            raise ValueError("Reserved manifest name exists in source")
                        safe_name(relative)
                        with path.open("rb") as member:
                            files[relative] = stream_digest(member)
                        if len(files) > MAX_FILES - 1 or sum(f["bytes"] for f in files.values()) > MAX_BYTES:
                            raise ValueError("Backup exceeds validation limits")
                        backup.write(path, relative)
                if "profiles.json" not in files:
                    raise ValueError("No committed profiles.json to back up")
                backup.writestr(MANIFEST, json.dumps({
                    "format": 1,
                    "createdAt": datetime.now(timezone.utc).isoformat(),
                    "consistency": "operator-confirmed-offline",
                    "files": files,
                }, indent=2))
            output.flush()
            os.fsync(output.fileno())
        sync_directory(archive.parent)
        return verify(archive)
    except BaseException:
        if created:
            archive.unlink(missing_ok=True)
        raise


def restore(archive, destination):
    manifest = verify(archive)
    destination = Path(destination).absolute()
    if destination.exists() or destination.is_symlink():
        raise ValueError("Restore destination must not exist")
    destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    staging = Path(tempfile.mkdtemp(prefix=".chao-restore-", dir=destination.parent))
    try:
        with zipfile.ZipFile(archive) as backup:
            for name in manifest["files"]:
                path = staging.joinpath(*safe_name(name).parts)
                path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                with path.open("xb") as output:
                    os.chmod(path, 0o600)
                    with backup.open(name) as member:
                        shutil.copyfileobj(member, output, length=1024 * 1024)
                    output.flush()
                    os.fsync(output.fileno())
                with path.open("rb") as member:
                    if stream_digest(member) != manifest["files"][name]:
                        raise ValueError("Archive changed during restore")
        for root, _, _ in os.walk(staging, topdown=False):
            sync_directory(root)
        # Same filesystem; publish the fully verified replacement as one rename.
        # No in-place overwrite: retain the old data directory for rollback.
        staging.rename(destination)
        sync_directory(destination.parent)
    finally:
        if staging.exists():
            shutil.rmtree(staging)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    make = sub.add_parser("create")
    make.add_argument("source")
    make.add_argument("archive")
    make.add_argument("--offline", action="store_true", required=True)
    check = sub.add_parser("verify")
    check.add_argument("archive")
    recover = sub.add_parser("restore")
    recover.add_argument("archive")
    recover.add_argument("destination")
    recover.add_argument("--offline", action="store_true", required=True)
    args = parser.parse_args()
    if args.command == "create":
        result = create(args.source, args.archive)
    elif args.command == "verify":
        result = verify(args.archive)
    else:
        result = restore(args.archive, args.destination)
    print(json.dumps({"verifiedFiles": len(result["files"]), "createdAt": result["createdAt"]}))


if __name__ == "__main__":
    main()
