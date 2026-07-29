"""Compare two ZIP-based release artifacts by normalized member content."""
from __future__ import annotations
import hashlib
import pathlib
import sys
import zipfile

def digest(path: pathlib.Path) -> tuple[tuple[str, str], ...]:
    with zipfile.ZipFile(path) as archive:
        members = {info.filename: hashlib.sha256(archive.read(info)).hexdigest() for info in archive.infolist() if not info.is_dir()}
    return tuple(sorted(members.items()))

def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: verify_reproducible.py <artifact-a> <artifact-b>")
    first, second = map(pathlib.Path, sys.argv[1:])
    if digest(first) != digest(second):
        raise SystemExit("release artifacts differ after normalizing archive metadata")
    print(f"Reproducible release content verified: {first.name} == {second.name}")

if __name__ == "__main__":
    main()
