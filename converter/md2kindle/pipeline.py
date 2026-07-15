from __future__ import annotations

import fnmatch
from dataclasses import dataclass
from pathlib import Path

from md2kindle.config import Config
from md2kindle.images import ImageCache
from md2kindle.layout import remove_output, write_html
from md2kindle.manifest import Manifest, hash_file
from md2kindle.preprocess import preprocess, strip_frontmatter
from md2kindle.renderer import render_markdown
from md2kindle.resolver import build_index


@dataclass
class SyncSummary:
    converted: int
    images_failed: int
    orphans_removed: int
    total: int


def scan_markdown(root: Path, exclude: list[str]) -> list[str]:
    rels: list[str] = []
    for p in sorted(root.rglob("*.md")):
        rel = p.relative_to(root).as_posix()
        name = Path(rel).name
        if any(fnmatch.fnmatch(rel, pat) or fnmatch.fnmatch(name, pat) for pat in exclude):
            continue
        rels.append(rel)
    return rels


def sync(cfg: Config, log=print, *, transport=None) -> SyncSummary:
    manifest_path = cfg.output_dir / "assets" / ".md2kindle" / "manifest.json"
    manifest = Manifest.load(manifest_path)

    sources = scan_markdown(cfg.input_dir, cfg.exclude)
    idx = build_index(sources)
    cache = ImageCache(
        cfg.output_dir / "assets", cfg.output_dir, cfg.input_dir, cfg.image, manifest, transport=transport
    )

    converted = images_failed = 0
    total = len(sources)
    for i, rel in enumerate(sources, 1):
        src = cfg.input_dir / rel
        h = hash_file(src)
        prev = manifest.get_source(rel)
        if prev and prev["hash"] == h and not prev.get("had_failures"):
            continue  # unchanged and no pending image retries

        print(f"[{i}/{total}] {rel}", flush=True)
        md = strip_frontmatter(src.read_text(encoding="utf-8"))
        md = preprocess(md, rel, idx, log)
        html = render_markdown(md, title=Path(rel).stem)
        html, n_fail = cache.process_html(html, rel)

        out_rel = write_html(cfg.output_dir, rel, html)
        manifest.set_source(rel, h, out_rel, had_failures=(n_fail > 0))
        converted += 1
        images_failed += n_fail

    # orphan cleanup: outputs whose source disappeared
    orphans = 0
    for rel in list(manifest.known_sources()):
        if rel not in set(sources):
            entry = manifest.get_source(rel)
            if entry:
                remove_output(cfg.output_dir, entry["output"])
                orphans += 1
            manifest.remove_source(rel)

    if manifest.dirty:
        manifest.save()
    return SyncSummary(converted, images_failed, orphans, len(sources))
