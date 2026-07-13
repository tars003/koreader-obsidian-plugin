from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path

from markdown.extensions.toc import slugify


@dataclass
class ResolveResult:
    href: str | None
    text: str
    broken: bool = False


@dataclass
class NoteIndex:
    by_basename: dict[str, list[str]]
    all_rels: list[str]


def build_index(relpaths: list[str]) -> NoteIndex:
    by_basename: dict[str, list[str]] = {}
    for rel in relpaths:
        stem = Path(rel).stem.lower()
        by_basename.setdefault(stem, []).append(rel)
    for k in by_basename:
        by_basename[k].sort(key=len)  # shortest path first (tiebreak)
    return NoteIndex(by_basename=by_basename, all_rels=list(relpaths))


def _rel_href(src_relpath: str, target_html_rel: str) -> str:
    src_dir = posix_dir(src_relpath)
    return os.path.relpath(target_html_rel, src_dir).replace(os.sep, "/")


def posix_dir(rel: str) -> str:
    return "/".join(rel.split("/")[:-1])


def resolve(target: str, idx: NoteIndex, src_relpath: str) -> ResolveResult:
    # 1. split alias
    if "|" in target:
        path_anchor, alias = target.split("|", 1)
        text = alias.strip()
    else:
        path_anchor, text = target, None
    path_anchor = path_anchor.strip()

    # 2. split anchor / block
    heading = None
    block = None
    if "#" in path_anchor:
        path_part, heading = path_anchor.split("#", 1)
    elif "^" in path_anchor:
        path_part, block = path_anchor.split("^", 1)
    else:
        path_part = path_anchor
    path_part = path_part.strip()

    # 3. same-file anchor only: [[#Heading]]
    if path_part == "":
        if heading:
            return ResolveResult(href="#" + slugify(heading, "-"), text=text or heading, broken=False)
        return ResolveResult(href=None, text=text or "", broken=True)

    # 4. candidates
    if "/" in path_part:
        needle = path_part.lower()
        cands = [
            r for r in idx.all_rels
            if r.lower() == needle
            or r.lower() == needle + ".md"
            or r.lower().endswith("/" + needle)
            or r.lower().endswith("/" + needle + ".md")
        ]
    else:
        cands = list(idx.by_basename.get(path_part.lower(), []))
    cands.sort(key=len)

    # 5. resolve
    if not cands:
        return ResolveResult(href=None, text=text or path_part, broken=True)

    chosen = cands[0]
    target_html_rel = Path(chosen).with_suffix(".html").as_posix()
    href = _rel_href(src_relpath, target_html_rel)
    if heading:
        href += "#" + slugify(heading, "-")
    # block refs (^id) are ignored in Phase 1 (link to note top)
    display = text if text is not None else Path(chosen).stem
    return ResolveResult(href=href, text=display, broken=False)
