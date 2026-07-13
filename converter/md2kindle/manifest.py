from __future__ import annotations

import hashlib
import json
from pathlib import Path


def hash_text(s: str) -> str:
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def hash_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def hash_file(path: Path) -> str:
    return hash_bytes(path.read_bytes())


class Manifest:
    def __init__(self, path: Path) -> None:
        self.path = path
        self.sources: dict[str, dict] = {}
        self.images: dict[str, dict] = {}
        self.dirty = False

    @classmethod
    def load(cls, path: Path) -> "Manifest":
        m = cls(path)
        if path.exists():
            data = json.loads(path.read_text(encoding="utf-8"))
            m.sources = data.get("sources", {})
            m.images = data.get("images", {})
        return m

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(self.path.suffix + ".tmp")
        tmp.write_text(
            json.dumps({"sources": self.sources, "images": self.images}, indent=2, sort_keys=True),
            encoding="utf-8",
        )
        tmp.replace(self.path)

    def get_source(self, rel: str) -> dict | None:
        return self.sources.get(rel)

    def set_source(self, rel: str, h: str, output: str, had_failures: bool = False) -> None:
        self.sources[rel] = {"hash": h, "output": output, "had_failures": had_failures}
        self.dirty = True

    def get_image(self, url: str) -> dict | None:
        return self.images.get(url)

    def set_image(self, url: str, h: str | None, file: str | None, failed: bool) -> None:
        self.images[url] = {"hash": h, "file": file, "failed": failed}
        self.dirty = True

    def known_sources(self) -> set[str]:
        return set(self.sources)

    def remove_source(self, rel: str) -> None:
        self.sources.pop(rel, None)
        self.dirty = True
