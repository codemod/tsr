"""Observe benchmark inputs without changing compiler behavior or claiming coverage."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import stat
from typing import BinaryIO


def path_identity(name: str, cwd: Path) -> str:
    # Preserve the exact spelling: alias/../file follows the symlink before
    # '..', and file/. or file/ is not a regular-file path. realpath is separate.
    return name if os.path.isabs(name) else str(cwd) + "/" + name


def load_manifest(path: Path | None, cwd: Path) -> tuple[list[str], dict]:
    if path is None:
        return [], {"provided": False, "coverage": "loaded sources and root config only"}
    with regular_file(path) as stream:
        raw = stream.read()
    value = json.loads(raw)
    if not isinstance(value, dict) or value.get("schema_version") != 1:
        raise ValueError("input manifest must be a schema_version 1 object")
    paths, provenance = value.get("paths"), value.get("provenance")
    if not isinstance(paths, list) or not all(isinstance(p, str) and p for p in paths):
        raise ValueError("input manifest paths must be nonempty path strings")
    if not isinstance(provenance, dict) or not all(
        isinstance(provenance.get(key), str) and provenance[key]
        for key in ("source_sha", "producer")
    ):
        raise ValueError("input manifest provenance requires source_sha and producer")
    return sorted({path_identity(p, cwd) for p in paths}), {
        "provided": True, "sha256": hashlib.sha256(raw).hexdigest(),
        "provenance": provenance, "coverage": "caller-supplied observed paths; partial",
    }


def file_kind(mode: int) -> str:
    if stat.S_ISREG(mode):
        return "file"
    if stat.S_ISDIR(mode):
        return "directory"
    if stat.S_ISLNK(mode):
        return "symlink"
    return "other"


def regular_file(path: str | Path) -> BinaryIO:
    # A path may change kind after stat. Open without blocking, then inspect the
    # actual descriptor before reading; a FIFO/device is never read as a file.
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    stream = os.fdopen(descriptor, "rb")
    if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
        stream.close()
        raise OSError("input is not a regular file")
    return stream


def file_hash(path: str | Path) -> str:
    with regular_file(path) as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode):
            raise OSError("input changed from regular file before open")
        digest = hashlib.sha256()
        while block := stream.read(1024 * 1024):
            digest.update(block)
        after = os.fstat(stream.fileno())
        fields = ("st_dev", "st_ino", "st_size", "st_mtime_ns", "st_ctime_ns")
        if any(getattr(before, key) != getattr(after, key) for key in fields):
            raise OSError("input changed during fingerprinting")
        return digest.hexdigest()


def snapshot(paths: list[str]) -> list[dict]:
    """Local-only rows. Directory entries include names/kinds and link spellings."""
    rows = []
    link_cache: dict[str, list[dict]] = {}

    def links(name: str) -> list[dict]:
        if name in link_cache:
            return link_cache[name]
        parent = os.path.dirname(name)
        result = [] if parent == name else list(links(parent))
        try:
            mode = os.lstat(name).st_mode
        except (FileNotFoundError, NotADirectoryError):
            pass
        else:
            if stat.S_ISLNK(mode):
                result.append({"path": name, "target": os.readlink(name)})
        link_cache[name] = result
        return result

    for name in sorted(set(paths)):
        row = {"path": name}
        try:
            row.update(realpath=os.path.realpath(name), symlinks=links(name))
            try:
                mode = os.stat(name).st_mode
            except (FileNotFoundError, NotADirectoryError):
                row["kind"] = "missing"
            else:
                kind = file_kind(mode)
                row["kind"] = kind
                if kind == "file":
                    row["sha256"] = file_hash(name)
                elif kind == "directory":
                    entries = []
                    with os.scandir(name) as directory:
                        for entry in directory:
                            item = {"name": entry.name,
                                    "kind": file_kind(entry.stat(follow_symlinks=False).st_mode)}
                            if item["kind"] == "symlink":
                                item["target"] = os.readlink(entry.path)
                            entries.append(item)
                    row["entries"] = sorted(entries, key=lambda item: item["name"])
        except (OSError, ValueError) as error:
            # Unreadable/racing inputs invalidate evidence; never call them missing.
            row["error"] = str(error)
        rows.append(row)
    return rows


def valid_snapshot(rows: list[dict]) -> bool:
    return all("error" not in row for row in rows)
