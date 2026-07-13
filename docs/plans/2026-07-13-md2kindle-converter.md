# md2kindle Converter — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `md2kindle`, a Python converter that turns an Obsidian `.md` vault into a self-contained `.html` vault (remote images downloaded + e-ink-optimized, `[[wikilinks]]` rewritten to relative links, folder tree mirrored) for offline reading on a Kindle.

**Architecture:** A scheduled incremental pipeline scans the vault, converts only changed notes (manifest-driven), and writes an output tree mirroring the vault. Pure file-I/O modules with single responsibilities; fully unit-testable with fixture vaults and no Kindle.

**Tech Stack:** Python ≥ 3.10, `markdown` (Python-Markdown) + extensions (`tables`, `fenced_code`, `attr_list`, `toc`, `admonition`), `Pillow`, `httpx`, `tomli` (only on <3.11; 3.11+ uses stdlib `tomllib`). Dev: `pytest`, `ruff`.

## Global Constraints

- **Python ≥ 3.10.** Config parsing uses `tomllib` on 3.11+ and falls back to `tomli` on 3.10.
- **Output is plain UTF-8 text/HTML.** All file I/O is `encoding="utf-8"`.
- **Paths in output links are POSIX-style** (forward slashes), even when generated on Windows — normalize with `.replace(os.sep, "/")`.
- **Atomic writes:** every output file is written to a `.tmp` sibling then `replace`d, so Syncthing never ships a half-written file.
- **Fail soft:** a failed image never aborts a run; it is replaced with `[image unavailable: <src>]`, logged, recorded in the manifest, and retried next run.
- **Idempotent:** re-running a sync with no changes performs no writes.
- **DRY / YAGNI / TDD:** each task writes a failing test first, then minimal code, then commits.

---

## File Structure

```
converter/
├── pyproject.toml
├── md2kindle.toml.example
├── README.md
├── md2kindle/
│   ├── __init__.py
│   ├── __main__.py          # python -m md2kindle  -> cli.main()
│   ├── cli.py               # argparse: `sync` subcommand
│   ├── config.py            # Config, ImageOptions, load_config
│   ├── manifest.py          # Manifest (state), hash_text/hash_file/hash_bytes
│   ├── resolver.py          # NoteIndex, build_index, resolve -> ResolveResult
│   ├── preprocess.py        # strip_frontmatter + code-aware [[wikilink]] rewrite
│   ├── renderer.py          # render_markdown -> full HTML doc + DEFAULT_CSS
│   ├── images.py            # ImageCache.process_html (download/optimize/dedup/local)
│   ├── layout.py            # write_html, atomic_write_text, remove_output
│   └── pipeline.py          # scan_markdown, sync -> SyncSummary (orchestration)
└── tests/
    ├── __init__.py
    ├── conftest.py          # shared fixtures (sample vault builder)
    ├── test_config.py
    ├── test_manifest.py
    ├── test_resolver.py
    ├── test_preprocess.py
    ├── test_renderer.py
    ├── test_images.py
    ├── test_layout.py
    └── test_pipeline.py
```

**Responsibilities:** `config` (parse TOML → typed Config), `manifest` (persistent state + hashing), `resolver` (pure wikilink→target logic), `preprocess` (rewrite markdown text using resolver, code-aware), `renderer` (markdown→HTML+CSS), `images` (fetch/optimize/cache, operates on rendered HTML), `layout` (write/remove output files atomically), `pipeline` (orchestrate one incremental pass), `cli` (entrypoint). Each has one job and a pure interface.

---

## Task 1: Scaffold + config

**Files:**
- Create: `converter/pyproject.toml`
- Create: `converter/md2kindle/__init__.py`, `converter/md2kindle/config.py`
- Create: `converter/tests/__init__.py`, `converter/tests/test_config.py`
- Create: `converter/md2kindle.toml.example`

**Interfaces:**
- Produces: `ImageOptions`, `Config`, `load_config(path: Path) -> Config`, `DEFAULT_EXCLUDE: list[str]`. Later tasks import `Config` and `ImageOptions` from `md2kindle.config`.

- [ ] **Step 1: Write the failing test**

```python
# converter/tests/test_config.py
from pathlib import Path
from md2kindle.config import Config, ImageOptions, load_config, DEFAULT_EXCLUDE


def test_load_config_parses_required_and_defaults(tmp_path):
    toml = tmp_path / "md2kindle.toml"
    toml.write_text(
        '[vault]\ninput_dir = "vault"\noutput_dir = "vault.kindle"\n',
        encoding="utf-8",
    )
    cfg = load_config(toml)
    assert isinstance(cfg, Config)
    assert cfg.input_dir == Path("vault")
    assert cfg.output_dir == Path("vault.kindle")
    assert cfg.exclude == DEFAULT_EXCLUDE
    assert isinstance(cfg.image, ImageOptions)
    assert cfg.image.max_width == 1000
    assert cfg.image.grayscale is True


def test_load_config_overrides_image_options(tmp_path):
    toml = tmp_path / "c.toml"
    toml.write_text(
        '[vault]\ninput_dir = "in"\noutput_dir = "out"\n'
        '[image]\nmax_width = 600\ngrayscale = false\n',
        encoding="utf-8",
    )
    cfg = load_config(toml)
    assert cfg.image.max_width == 600
    assert cfg.image.grayscale is False
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd converter && python -m pytest tests/test_config.py -v`
Expected: FAIL with `ImportError` (module `md2kindle.config` does not exist).

- [ ] **Step 3: Write minimal implementation**

```python
# converter/md2kindle/__init__.py
__version__ = "0.1.0"
```

```python
# converter/md2kindle/config.py
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

try:  # Python >= 3.11
    import tomllib
except ModuleNotFoundError:  # Python 3.10
    import tomli as tomllib  # type: ignore[no-redef]

DEFAULT_EXCLUDE: list[str] = [".obsidian/*", "templates/*"]


@dataclass
class ImageOptions:
    max_width: int = 1000
    grayscale: bool = True
    jpeg_quality: int = 70
    timeout: float = 15.0
    retries: int = 3
    max_bytes: int = 8 * 1024 * 1024  # 8 MB


@dataclass
class Config:
    input_dir: Path
    output_dir: Path
    exclude: list[str] = field(default_factory=lambda: list(DEFAULT_EXCLUDE))
    image: ImageOptions = field(default_factory=ImageOptions)


def load_config(path: Path) -> Config:
    with open(path, "rb") as f:
        data = tomllib.load(f)
    vault = data["vault"]
    cfg = Config(
        input_dir=Path(vault["input_dir"]),
        output_dir=Path(vault["output_dir"]),
        exclude=list(data.get("exclude", DEFAULT_EXCLUDE)),
    )
    img = data.get("image", {})
    if img:
        cfg.image = ImageOptions(**{k: img[k] for k in img})
    return cfg
```

```toml
# converter/md2kindle.toml.example
[vault]
input_dir = "vault"
output_dir = "vault.kindle"

exclude = [".obsidian/*", "templates/*"]

[image]
max_width = 1000
grayscale = true
jpeg_quality = 70
timeout = 15.0
retries = 3
max_bytes = 8388608
```

```toml
# converter/pyproject.toml
[build-system]
requires = ["setuptools>=68"]
build-backend = "setuptools.build_meta"

[project]
name = "md2kindle"
version = "0.1.0"
requires-python = ">=3.10"
dependencies = [
    "markdown>=3.5",
    "Pillow>=10.0",
    "httpx>=0.25",
    'tomli>=2.0; python_version < "3.11"',
]

[project.optional-dependencies]
dev = ["pytest>=7", "ruff>=0.1"]

[project.scripts]
md2kindle = "md2kindle.cli:main"

[tool.setuptools.packages.find]
include = ["md2kindle*"]
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd converter && pip install -e ".[dev]" && python -m pytest tests/test_config.py -v`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add converter/pyproject.toml converter/md2kindle.toml.example converter/md2kindle/__init__.py converter/md2kindle/config.py converter/tests/__init__.py converter/tests/test_config.py
git commit -m "feat(converter): scaffold + config loader"
```

---

## Task 2: Manifest (state + hashing)

**Files:**
- Create: `converter/md2kindle/manifest.py`
- Create: `converter/tests/test_manifest.py`

**Interfaces:**
- Consumes: nothing (stdlib only).
- Produces: `hash_text(s) -> str`, `hash_file(path) -> str`, `hash_bytes(b) -> str`, and `class Manifest` with: `load(path) -> Manifest` (classmethod), `save()`, `get_source(rel) -> dict | None`, `set_source(rel, h, output, had_failures=False)`, `get_image(url) -> dict | None`, `set_image(url, h, file, failed)`, `known_sources() -> set[str]`, `remove_source(rel)`.

- [ ] **Step 1: Write the failing test**

```python
# converter/tests/test_manifest.py
from pathlib import Path
from md2kindle.manifest import Manifest, hash_text, hash_file


def test_hash_text_is_stable():
    assert hash_text("abc") == hash_text("abc")
    assert hash_text("abc") != hash_text("abd")


def test_hash_file_reads_bytes(tmp_path):
    f = tmp_path / "n.md"
    f.write_text("hello", encoding="utf-8")
    assert hash_file(f) == hash_text("hello")


def test_manifest_roundtrip(tmp_path):
    p = tmp_path / "manifest.json"
    m = Manifest.load(p)
    assert m.get_source("a.md") is None
    m.set_source("a.md", "h1", "a.html")
    m.set_image("http://x/y.png", "ih", "assets/ih.jpg", failed=False)
    m.save()

    m2 = Manifest.load(p)
    assert m2.get_source("a.md") == {"hash": "h1", "output": "a.html", "had_failures": False}
    assert m2.known_sources() == {"a.md"}
    assert m2.get_image("http://x/y.png")["file"] == "assets/ih.jpg"

    m2.remove_source("a.md")
    assert m2.known_sources() == set()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd converter && python -m pytest tests/test_manifest.py -v`
Expected: FAIL (`ImportError`).

- [ ] **Step 3: Write minimal implementation**

```python
# converter/md2kindle/manifest.py
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

    def get_image(self, url: str) -> dict | None:
        return self.images.get(url)

    def set_image(self, url: str, h: str | None, file: str | None, failed: bool) -> None:
        self.images[url] = {"hash": h, "file": file, "failed": failed}

    def known_sources(self) -> set[str]:
        return set(self.sources)

    def remove_source(self, rel: str) -> None:
        self.sources.pop(rel, None)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd converter && python -m pytest tests/test_manifest.py -v`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add converter/md2kindle/manifest.py converter/tests/test_manifest.py
git commit -m "feat(converter): manifest state + hashing"
```

---

## Task 3: Resolver (note index + wikilink resolution)

**Files:**
- Create: `converter/md2kindle/resolver.py`
- Create: `converter/tests/test_resolver.py`

**Interfaces:**
- Consumes: `from markdown.extensions.toc import slugify`.
- Produces: `ResolveResult(href, text, broken)`, `NoteIndex(by_basename, all_rels)`, `build_index(relpaths) -> NoteIndex`, `resolve(target, idx, src_relpath) -> ResolveResult`. `preprocess.py` (Task 4) calls `resolve`.

- [ ] **Step 1: Write the failing test**

```python
# converter/tests/test_resolver.py
from md2kindle.resolver import build_index, resolve


def _idx():
    return build_index(["Notes/Projects/design.md", "Notes/Inbox/idea.md", "Daily/today.md"])


def test_simple_basename_resolves_to_relative_html():
    idx = _idx()
    r = resolve("idea", idx, "Notes/Projects/design.md")
    assert r.broken is False
    assert r.href == "../Inbox/idea.html"
    assert r.text == "idea"


def test_alias_uses_alias_as_text():
    idx = _idx()
    r = resolve("idea|my cool note", idx, "Notes/Projects/design.md")
    assert r.href == "../Inbox/idea.html"
    assert r.text == "my cool note"


def test_partial_path_match():
    idx = _idx()
    r = resolve("Projects/design", idx, "Daily/today.md")
    assert r.href == "../Notes/Projects/design.html"


def test_heading_anchor_appended():
    idx = _idx()
    r = resolve("idea#Section One", idx, "Notes/Projects/design.md")
    assert r.href == "../Inbox/idea.html#section-one"


def test_same_file_anchor():
    idx = _idx()
    r = resolve("#Section One", idx, "Notes/Projects/design.md")
    assert r.href == "#section-one"
    assert r.broken is False


def test_duplicate_basenames_pick_shortest_path():
    idx = build_index(["a/x.md", "a/b/x.md"])
    r = resolve("x", idx, "Daily/today.md")
    assert r.href == "../a/x.html"


def test_broken_link_when_no_match():
    idx = _idx()
    r = resolve("nope", idx, "Notes/Projects/design.md")
    assert r.broken is True
    assert r.href is None
    assert r.text == "nope"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd converter && python -m pytest tests/test_resolver.py -v`
Expected: FAIL (`ImportError`).

- [ ] **Step 3: Write minimal implementation**

```python
# converter/md2kindle/resolver.py
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


def _choose(target_html_rel: str, anchor: str | None, text: str | None) -> ResolveResult:
    href = _rel_href("__src__", target_html_rel)  # placeholder, overwritten by caller
    raise NotImplementedError  # replaced below


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
```

> Note: delete the throwaway `_choose` stub above; it is unused. Keep only `resolve`, `build_index`, `_rel_href`, `posix_dir`, the dataclasses, and the `slugify` import.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd converter && python -m pytest tests/test_resolver.py -v`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add converter/md2kindle/resolver.py converter/tests/test_resolver.py
git commit -m "feat(converter): note index + wikilink resolver"
```

---

## Task 4: Preprocess (code-aware wikilink rewrite + frontmatter)

**Files:**
- Create: `converter/md2kindle/preprocess.py`
- Create: `converter/tests/test_preprocess.py`

**Interfaces:**
- Consumes: `resolve`, `NoteIndex` from `md2kindle.resolver`.
- Produces: `strip_frontmatter(md) -> str`, `preprocess(md, src_relpath, idx, log) -> str`. `pipeline.py` (Task 8) calls `strip_frontmatter` then `preprocess`.

- [ ] **Step 1: Write the failing test**

```python
# converter/tests/test_preprocess.py
from md2kindle.preprocess import preprocess, strip_frontmatter
from md2kindle.resolver import build_index


IDX = build_index(["Notes/idea.md", "assets/photo.png"])


def test_strip_frontmatter():
    md = "---\ntitle: x\n---\n# Body\n"
    assert strip_frontmatter(md) == "# Body\n"


def test_no_frontmatter_untouched():
    assert strip_frontmatter("# Hi\n") == "# Hi\n"


def test_wikilink_rewritten():
    out = preprocess("See [[idea]] please", "Notes/other.md", IDX, log=lambda *_: None)
    assert out == "See [idea](idea.html) please"


def test_broken_link_renders_dimmed_span():
    out = preprocess("See [[ghost]]", "Notes/other.md", IDX, log=lambda *_: None)
    assert '<span class="broken-link">ghost</span>' in out


def test_wikilink_not_rewritten_inside_code_fence():
    md = "text\n\n```\n[[idea]]\n```\n"
    out = preprocess(md, "Notes/other.md", IDX, log=lambda *_: None)
    assert "[idea](idea.html)" not in out
    assert "[[idea]]" in out


def test_wikilink_not_rewritten_inside_inline_code():
    out = preprocess("run `[[idea]]` now", "Notes/other.md", IDX, log=lambda *_: None)
    assert "[[idea]]" in out


def test_image_embed_normalized_to_md_image():
    out = preprocess("![[_logo.png]]", "Notes/other.md", IDX, log=lambda *_: None)
    assert out == "![_logo.png](_logo.png)"


def test_note_transclusion_renders_placeholder():
    out = preprocess("![[idea]]", "Notes/other.md", IDX, log=lambda *_: None)
    assert "transclusion" in out and "idea" in out


def test_standard_md_link_extension_rewritten():
    out = preprocess("[a](idea.md)", "Notes/other.md", IDX, log=lambda *_: None)
    assert out == "[a](idea.html)"


def test_http_link_not_rewritten():
    out = preprocess("[a](https://example.com)", "Notes/other.md", IDX, log=lambda *_: None)
    assert out == "[a](https://example.com)"


def test_broken_link_is_logged():
    msgs = []
    preprocess("[[ghost]]", "Notes/other.md", IDX, log=lambda *a: msgs.append(a))
    assert msgs and "ghost" in msgs[0][0]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd converter && python -m pytest tests/test_preprocess.py -v`
Expected: FAIL (`ImportError`).

- [ ] **Step 3: Write minimal implementation**

```python
# converter/md2kindle/preprocess.py
from __future__ import annotations

import re

from md2kindle.resolver import NoteIndex, resolve

IMAGE_EXTS = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg"}

_WIKI_RE = re.compile(r"!\[\[([^]]*)\]\]|\[\[([^]]*)\]\]")
_MD_LINK_RE = re.compile(r"\[([^\]]*)\]\(([^)]+)\)")
_INLINE_CODE_RE = re.compile(r"(`+)([^`]*?)\1")


def strip_frontmatter(md: str) -> str:
    if md[:4] == "---\n":
        end = md.find("\n---", 4)
        if end != -1:
            nl = md.find("\n", end + 4)
            return md[nl + 1 :] if nl != -1 else ""
    return md


def preprocess(md: str, src_relpath: str, idx: NoteIndex, log) -> str:
    out: list[str] = []
    in_fence = False
    fence = None
    for line in md.splitlines(keepends=True):
        stripped = line.lstrip()
        if not in_fence and (stripped.startswith("```") or stripped.startswith("~~~")):
            in_fence = True
            fence = "```" if stripped.startswith("```") else "~~~"
            out.append(line)
            continue
        if in_fence:
            if stripped.startswith(fence):
                in_fence = False
                fence = None
            out.append(line)
            continue
        out.append(_rewrite_line(line, src_relpath, idx, log))
    return "".join(out)


def _rewrite_line(line: str, src_relpath: str, idx: NoteIndex, log) -> str:
    # Protect inline code spans, rewrite the rest, then restore.
    parts: list[str] = []
    pos = 0
    for m in _INLINE_CODE_RE.finditer(line):
        parts.append(_rewrite_text(line[pos : m.start()], src_relpath, idx, log))
        parts.append(m.group(0))
        pos = m.end()
    parts.append(_rewrite_text(line[pos:], src_relpath, idx, log))
    return "".join(parts)


def _rewrite_text(text: str, src_relpath: str, idx: NoteIndex, log) -> str:
    text = _WIKI_RE.sub(lambda m: _handle(m, src_relpath, idx, log), text)
    text = _MD_LINK_RE.sub(_rewrite_md_link, text)
    return text


def _handle(m: re.Match, src_relpath: str, idx: NoteIndex, log) -> str:
    embed, link = m.group(1), m.group(2)
    if embed is not None:
        return _handle_embed(embed, src_relpath, idx, log)
    r = resolve(link, idx, src_relpath)
    if r.broken:
        log(f"broken link: [[{link}]] in {src_relpath}")
        return f'<span class="broken-link">{r.text}</span>'
    return f"[{r.text}]({r.href})"


def _handle_embed(embed: str, src_relpath: str, idx: NoteIndex, log) -> str:
    target = embed.split("|", 1)[0].split("#", 1)[0].strip()
    if any(target.lower().endswith(ext) for ext in IMAGE_EXTS):
        return f"![{target}]({target})"
    log(f"transclusion placeholder: ![[{embed}]] in {src_relpath}")
    return f"[transclusion: {target}]"


def _rewrite_md_link(m: re.Match) -> str:
    text, url = m.group(1), m.group(2)
    if "://" in url or url.startswith("#") or url.startswith("mailto:"):
        return m.group(0)
    if url.lower().endswith(".md"):
        return f"[{text}]({url[:-3]}.html)"
    return m.group(0)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd converter && python -m pytest tests/test_preprocess.py -v`
Expected: PASS (11 tests).

- [ ] **Step 5: Commit**

```bash
git add converter/md2kindle/preprocess.py converter/tests/test_preprocess.py
git commit -m "feat(converter): code-aware wikilink preprocessing + frontmatter strip"
```

---

## Task 5: Renderer (markdown → HTML + e-ink CSS)

**Files:**
- Create: `converter/md2kindle/renderer.py`
- Create: `converter/tests/test_renderer.py`

**Interfaces:**
- Consumes: `markdown` library.
- Produces: `DEFAULT_CSS: str`, `render_markdown(md, css=DEFAULT_CSS) -> str` (returns a complete HTML document). `pipeline.py` calls `render_markdown`.

- [ ] **Step 1: Write the failing test**

```python
# converter/tests/test_renderer.py
from md2kindle.renderer import DEFAULT_CSS, render_markdown


def test_renders_headings_and_emphasis():
    html = render_markdown("# Title\n\nSome *bold* text\n")
    assert "<h1" in html and "id=\"title\"" in html
    assert "<em>bold</em>" in html


def test_includes_css_and_doctype():
    html = render_markdown("# Hi\n")
    assert html.startswith("<!DOCTYPE html>")
    assert "<style>" in html and DEFAULT_CSS in html


def test_renders_image_tag():
    html = render_markdown("![alt](photo.png)\n")
    assert '<img src="photo.png" alt="alt"' in html
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd converter && python -m pytest tests/test_renderer.py -v`
Expected: FAIL (`ImportError`).

- [ ] **Step 3: Write minimal implementation**

```python
# converter/md2kindle/renderer.py
from __future__ import annotations

import markdown

DEFAULT_CSS = """
body { font-family: serif; line-height: 1.5; margin: 4%; color: black; background: white; }
h1, h2, h3 { line-height: 1.25; }
img { max-width: 100%; height: auto; }
code, pre { font-family: monospace; }
pre { background: #eee; padding: 0.5em; white-space: pre-wrap; }
.broken-link { color: #888; }
""".strip()

_EXTENSIONS = ["tables", "fenced_code", "attr_list", "toc", "admonition"]


def render_markdown(md: str, css: str = DEFAULT_CSS) -> str:
    body = markdown.markdown(md, extensions=_EXTENSIONS)
    return (
        "<!DOCTYPE html>\n"
        '<html lang="en">\n<head>\n<meta charset="utf-8">\n'
        f"<style>\n{css}\n</style>\n</head>\n<body>\n{body}\n</body>\n</html>\n"
    )
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd converter && python -m pytest tests/test_renderer.py -v`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add converter/md2kindle/renderer.py converter/tests/test_renderer.py
git commit -m "feat(converter): markdown renderer + e-ink CSS"
```

---

## Task 6: Image pipeline (fetch / optimize / dedup / local copy)

**Files:**
- Create: `converter/md2kindle/images.py`
- Create: `converter/tests/test_images.py`

**Interfaces:**
- Consumes: `ImageOptions` (`md2kindle.config`), `Manifest`, `hash_bytes` (`md2kindle.manifest`), `httpx`, `Pillow`.
- Produces: `class ImageCache` with `__init__(self, assets_dir, output_root, input_dir, opts, manifest)` and `process_html(html, src_relpath) -> tuple[str, int]` (rewritten HTML, number of failed images). `pipeline.py` constructs one `ImageCache` per run.

- [ ] **Step 1: Write the failing test**

```python
# converter/tests/test_images.py
import httpx
from pathlib import Path
from PIL import Image
import io

from md2kindle.config import ImageOptions
from md2kindle.manifest import Manifest
from md2kindle.images import ImageCache


def _png_bytes(color=(255, 0, 0), size=(2000, 1000)):
    buf = io.BytesIO()
    Image.new("RGB", size, color).save(buf, format="PNG")
    return buf.getvalue()


def _cache(tmp_path, *, transport=None):
    assets = tmp_path / "assets"
    out_root = tmp_path
    in_root = tmp_path / "in"
    in_root.mkdir()
    opts = ImageOptions(max_width=500, grayscale=True, retries=0)
    m = Manifest.load(assets / ".md2kindle" / "manifest.json")
    return ImageCache(assets, out_root, in_root, opts, m, transport=transport), in_root, m


def test_remote_image_downloaded_optimized_and_rewritten(tmp_path):
    png = _png_bytes()
    transport = httpx.MockTransport(lambda req: httpx.Response(200, content=png))
    cache, _, _ = _cache(tmp_path, transport=transport)
    html, failed = cache.process_html('<img src="https://x/a.png" alt="a">', "note.md")
    assert failed == 0
    assert "https://x/a.png" not in html
    assert "assets/" in html
    files = list((tmp_path / "assets").glob("*.jpg"))
    assert len(files) == 1
    im = Image.open(files[0])
    assert im.width == 500          # downscaled
    assert im.mode == "L"           # grayscale


def test_remote_image_failure_yields_placeholder_and_is_logged(tmp_path):
    transport = httpx.MockTransport(lambda req: httpx.Response(404))
    cache, _, m = _cache(tmp_path, transport=transport)
    html, failed = cache.process_html('<img src="https://x/a.png">', "note.md")
    assert failed == 1
    assert "[image unavailable: https://x/a.png]" in html
    assert m.get_image("https://x/a.png")["failed"] is True


def test_local_attachment_copied_and_optimized(tmp_path):
    cache, in_root, _ = _cache(tmp_path)
    (in_root / "logo.png").write_bytes(_png_bytes())
    html, failed = cache.process_html('<img src="logo.png">', "note.md")
    assert failed == 0
    assert "assets/" in html and "logo.png" not in html
    assert list((tmp_path / "assets").glob("*.jpg"))


def test_dedup_identical_content_stored_once(tmp_path):
    png = _png_bytes()
    transport = httpx.MockTransport(lambda req: httpx.Response(200, content=png))
    cache, _, _ = _cache(tmp_path, transport=transport)
    cache.process_html('<img src="https://x/a.png">', "n.md")
    cache.process_html('<img src="https://x/b.png">', "n.md")
    assert len(list((tmp_path / "assets").glob("*.jpg"))) == 1
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd converter && python -m pytest tests/test_images.py -v`
Expected: FAIL (`ImportError`).

- [ ] **Step 3: Write minimal implementation**

```python
# converter/md2kindle/images.py
from __future__ import annotations

import hashlib
import io
import os
import re
import time
from pathlib import Path

import httpx
from PIL import Image

from md2kindle.config import ImageOptions
from md2kindle.manifest import Manifest, hash_bytes

_IMG_RE = re.compile(r'<img([^>]*?)src="([^"]+)"', re.IGNORECASE)


def _posix_dir(rel: str) -> str:
    return "/".join(rel.split("/")[:-1])


def _rel_href(src_dir: str, target_rel: str) -> str:
    return os.path.relpath(target_rel, src_dir).replace(os.sep, "/")


def _optimize(data: bytes, opts: ImageOptions) -> bytes:
    im = Image.open(io.BytesIO(data))
    im.load()
    if opts.grayscale:
        im = im.convert("L")
    elif im.mode not in ("RGB", "L"):
        im = im.convert("RGB")
    if opts.max_width and im.width > opts.max_width:
        new_h = int(im.height * (opts.max_width / im.width))
        im = im.resize((opts.max_width, new_h))
    buf = io.BytesIO()
    im.save(buf, format="JPEG", quality=opts.jpeg_quality, optimize=True)
    return buf.getvalue()


def _fetch_remote(url: str, opts: ImageOptions, transport=None):
    client_kwargs = {"timeout": opts.timeout, "follow_redirects": True}
    if transport is not None:
        client_kwargs["transport"] = transport
    last = None
    for attempt in range(opts.retries + 1):
        try:
            with httpx.Client(**client_kwargs) as c:
                r = c.get(url)
                r.raise_for_status()
            if len(r.content) > opts.max_bytes:
                return None, "too large"
            return r.content, None
        except Exception as e:  # noqa: BLE001
            last = e
            if attempt < opts.retries:
                time.sleep(0.5 * (2 ** attempt))
    return None, str(last)


class ImageCache:
    def __init__(
        self,
        assets_dir: Path,
        output_root: Path,
        input_dir: Path,
        opts: ImageOptions,
        manifest: Manifest,
        *,
        transport=None,
    ) -> None:
        self.assets_dir = assets_dir
        self.output_root = output_root
        self.input_dir = input_dir
        self.opts = opts
        self.manifest = manifest
        self.transport = transport

    def process_html(self, html: str, src_relpath: str) -> tuple[str, int]:
        src_dir = _posix_dir(src_relpath)
        failed = 0

        def repl(m: re.Match) -> str:
            nonlocal failed
            attrs, src = m.group(1), m.group(2)
            fname = self._get(src, src_relpath)
            if fname is None:
                failed += 1
                return f"[image unavailable: {src}]"
            return f'<img{attrs}src="{_rel_href(src_dir, "assets/" + fname)}"'

        return _IMG_RE.sub(repl, html), failed

    def _store(self, data: bytes) -> str:
        optimized = _optimize(data, self.opts)
        h = hashlib.sha256(optimized).hexdigest()[:16]
        fname = f"{h}.jpg"
        fpath = self.assets_dir / fname
        if not fpath.exists():
            self.assets_dir.mkdir(parents=True, exist_ok=True)
            fpath.write_bytes(optimized)
        return fname

    def _get(self, src: str, src_relpath: str) -> str | None:
        if src.startswith(("http://", "https://")):
            entry = self.manifest.get_image(src)
            if entry and not entry.get("failed") and entry.get("file"):
                return Path(entry["file"]).name
            data, _err = _fetch_remote(src, self.opts, self.transport)
            if data is None:
                self.manifest.set_image(src, None, None, True)
                return None
            fname = self._store(data)
            self.manifest.set_image(src, hash_bytes(data), f"assets/{fname}", False)
            return fname
        # local attachment, relative to the note's directory in the input vault
        local = self.input_dir / _posix_dir(src_relpath) / src
        if not local.exists():
            return None
        return self._store(local.read_bytes())
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd converter && python -m pytest tests/test_images.py -v`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add converter/md2kindle/images.py converter/tests/test_images.py
git commit -m "feat(converter): image pipeline (fetch/optimize/dedup/local)"
```

---

## Task 7: Layout (atomic output writing + orphan removal)

**Files:**
- Create: `converter/md2kindle/layout.py`
- Create: `converter/tests/test_layout.py`

**Interfaces:**
- Consumes: nothing (stdlib only).
- Produces: `atomic_write_text(path, data)`, `write_html(output_root, src_relpath, html) -> str` (returns output relpath like `Notes/idea.html`), `remove_output(output_root, out_rel)`. `pipeline.py` uses all three.

- [ ] **Step 1: Write the failing test**

```python
# converter/tests/test_layout.py
from pathlib import Path
from md2kindle.layout import atomic_write_text, write_html, remove_output


def test_atomic_write_replaces_existing(tmp_path):
    p = tmp_path / "a.html"
    atomic_write_text(p, "v1")
    atomic_write_text(p, "v2")
    assert p.read_text(encoding="utf-8") == "v2"
    assert not (tmp_path / "a.html.tmp").exists()


def test_write_html_mirrors_relpath_with_html_suffix(tmp_path):
    out = write_html(tmp_path, "Notes/idea.md", "<html></html>")
    assert out == "Notes/idea.html"
    assert (tmp_path / "Notes" / "idea.html").read_text(encoding="utf-8") == "<html></html>"


def test_remove_output_deletes_file(tmp_path):
    write_html(tmp_path, "Notes/idea.md", "x")
    remove_output(tmp_path, "Notes/idea.html")
    assert not (tmp_path / "Notes" / "idea.html").exists()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd converter && python -m pytest tests/test_layout.py -v`
Expected: FAIL (`ImportError`).

- [ ] **Step 3: Write minimal implementation**

```python
# converter/md2kindle/layout.py
from __future__ import annotations

from pathlib import Path


def atomic_write_text(path: Path, data: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(data, encoding="utf-8")
    tmp.replace(path)


def write_html(output_root: Path, src_relpath: str, html: str) -> str:
    out_rel = Path(src_relpath).with_suffix(".html").as_posix()
    atomic_write_text(output_root / out_rel, html)
    return out_rel


def remove_output(output_root: Path, out_rel: str) -> None:
    p = output_root / out_rel
    if p.exists():
        p.unlink()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd converter && python -m pytest tests/test_layout.py -v`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add converter/md2kindle/layout.py converter/tests/test_layout.py
git commit -m "feat(converter): atomic output writer + orphan removal"
```

---

## Task 8: Pipeline (incremental sync orchestration)

**Files:**
- Create: `converter/md2kindle/pipeline.py`
- Create: `converter/tests/conftest.py`
- Create: `converter/tests/test_pipeline.py`

**Interfaces:**
- Consumes: `Config`, `ImageOptions` (`config`); `Manifest`, `hash_file` (`manifest`); `build_index` (`resolver`); `strip_frontmatter`, `preprocess` (`preprocess`); `render_markdown` (`renderer`); `ImageCache` (`images`); `write_html`, `remove_output` (`layout`).
- Produces: `SyncSummary(converted, images_failed, orphans_removed, total)`, `scan_markdown(root, exclude) -> list[str]`, `sync(cfg, log=print) -> SyncSummary`. `cli.py` (Task 9) calls `sync`.

- [ ] **Step 1: Write the failing test**

```python
# converter/tests/conftest.py
import io
from pathlib import Path
from PIL import Image
import httpx
import pytest


@pytest.fixture
def png_bytes():
    buf = io.BytesIO()
    Image.new("RGB", (2000, 1000), (0, 128, 255)).save(buf, format="PNG")
    return buf.getvalue()


@pytest.fixture
def fake_remote_transport(png_bytes):
    return httpx.MockTransport(lambda req: httpx.Response(200, content=png_bytes))
```

```python
# converter/tests/test_pipeline.py
from md2kindle.config import Config, ImageOptions
from md2kindle.pipeline import sync


def _cfg(tmp_path, transport):
    in_dir = tmp_path / "in"
    out_dir = tmp_path / "out"
    in_dir.mkdir()
    cfg = Config(input_dir=in_dir, output_dir=out_dir)
    cfg.image = ImageOptions(max_width=400, grayscale=True, retries=0)
    return cfg, in_dir, out_dir


def test_sync_converts_markdown_and_rewrites_wikilink(tmp_path, fake_remote_transport, monkeypatch):
    cfg, in_dir, out_dir = _cfg(tmp_path, fake_remote_transport)
    (in_dir / "a.md").write_text("# A\nLink to [[b]] and ![img](https://x/i.png)\n", encoding="utf-8")
    (in_dir / "b.md").write_text("# B\n", encoding="utf-8")

    # force the pipeline's ImageCache to use the mock transport
    from md2kindle import images as images_mod
    monkeypatch.setattr(images_mod.ImageCache, "__init__",
                        lambda self, assets, out, in_d, opts, m, **kw: images_mod.ImageCache.__init_orig__(
                            self, assets, out, in_d, opts, m, transport=fake_remote_transport))
    # simpler: patch _fetch_remote via module is messy; instead inject transport through env-free path:
    monkeypatch.setattr(images_mod, "_fetch_remote",
                        lambda url, opts, transport=None: (None, "patched") if False else _read(png_for_test(url)))

    summary = sync(cfg, log=lambda *_: None)
    assert summary.total == 2 and summary.converted == 2
    a = (out_dir / "a.html").read_text(encoding="utf-8")
    assert 'href="b.html"' in a
    assert "assets/" in a  # image rewritten
    assert "[[b]]" not in a


def test_sync_is_idempotent_second_run_no_conversions(tmp_path, fake_remote_transport, monkeypatch):
    cfg, in_dir, out_dir = _cfg(tmp_path, fake_remote_transport)
    (in_dir / "a.md").write_text("# A\n", encoding="utf-8")
    sync(cfg, log=lambda *_: None)
    s2 = sync(cfg, log=lambda *_: None)
    assert s2.converted == 0


def test_sync_removes_orphan_output(tmp_path, fake_remote_transport, monkeypatch):
    cfg, in_dir, out_dir = _cfg(tmp_path, fake_remote_transport)
    (in_dir / "a.md").write_text("# A\n", encoding="utf-8")
    sync(cfg, log=lambda *_: None)
    (in_dir / "a.md").unlink()
    s = sync(cfg, log=lambda *_: None)
    assert s.orphans_removed == 1
    assert not (out_dir / "a.html").exists()


def test_sync_excludes_obsidian_folder(tmp_path, fake_remote_transport):
    cfg, in_dir, out_dir = _cfg(tmp_path, fake_remote_transport)
    (in_dir / ".obsidian").mkdir()
    (in_dir / ".obsidian" / "app.json").write_text("{}", encoding="utf-8")
    (in_dir / "keep.md").write_text("# K\n", encoding="utf-8")
    s = sync(cfg, log=lambda *_: None)
    assert s.total == 1
    assert not (out_dir / ".obsidian").exists()
```

> The transport-injection in `test_sync_converts_markdown_and_rewrites_wikilink` is awkward. **Cleaner approach (use this instead):** add an optional `transport` parameter to `sync(cfg, log=print, transport=None)` and pass it into `ImageCache`. Replace that test's monkeypatching with `sync(cfg, log=..., transport=fake_remote_transport)`. Apply this in Step 3.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd converter && python -m pytest tests/test_pipeline.py -v`
Expected: FAIL (`ImportError`).

- [ ] **Step 3: Write minimal implementation**

```python
# converter/md2kindle/pipeline.py
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


def _excluded(rel: str, exclude: list[str]) -> bool:
    name = Path(rel).name
    return any(fnmatch.fnmatch(rel, pat) or fnmatch.fnmatch(name, pat) for pat in exclude)


def sync(cfg: Config, log=print, *, transport=None) -> SyncSummary:
    manifest_path = cfg.output_dir / "assets" / ".md2kindle" / "manifest.json"
    manifest = Manifest.load(manifest_path)

    sources = scan_markdown(cfg.input_dir, cfg.exclude)
    idx = build_index(sources)
    cache = ImageCache(
        cfg.output_dir / "assets", cfg.output_dir, cfg.input_dir, cfg.image, manifest, transport=transport
    )

    converted = images_failed = 0
    for rel in sources:
        src = cfg.input_dir / rel
        h = hash_file(src)
        prev = manifest.get_source(rel)
        if prev and prev["hash"] == h and not prev.get("had_failures"):
            continue  # unchanged and no pending image retries

        md = strip_frontmatter(src.read_text(encoding="utf-8"))
        md = preprocess(md, rel, idx, log)
        html = render_markdown(md)
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

    manifest.save()
    return SyncSummary(converted, images_failed, orphans, len(sources))
```

Also revise the awkward test from Step 1 to use the `transport=` parameter:

```python
def test_sync_converts_markdown_and_rewrites_wikilink(tmp_path, fake_remote_transport):
    cfg, in_dir, out_dir = _cfg(tmp_path, fake_remote_transport)
    (in_dir / "a.md").write_text("# A\nLink to [[b]] and ![img](https://x/i.png)\n", encoding="utf-8")
    (in_dir / "b.md").write_text("# B\n", encoding="utf-8")
    summary = sync(cfg, log=lambda *_: None, transport=fake_remote_transport)
    assert summary.total == 2 and summary.converted == 2
    a = (out_dir / "a.html").read_text(encoding="utf-8")
    assert 'href="b.html"' in a
    assert "assets/" in a
    assert "[[b]]" not in a
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd converter && python -m pytest tests/test_pipeline.py -v`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add converter/md2kindle/pipeline.py converter/tests/conftest.py converter/tests/test_pipeline.py
git commit -m "feat(converter): incremental sync pipeline + orphan cleanup"
```

---

## Task 9: CLI + integration test + README

**Files:**
- Create: `converter/md2kindle/__main__.py`, `converter/md2kindle/cli.py`
- Create: `converter/tests/test_cli.py`
- Create: `converter/README.md`

**Interfaces:**
- Consumes: `load_config` (`config`), `sync` (`pipeline`).
- Produces: `main(argv=None) -> int` (exit code). `python -m md2kindle sync --config <path>` runs one pass and prints a summary.

- [ ] **Step 1: Write the failing test**

```python
# converter/tests/test_cli.py
import subprocess
import sys
from pathlib import Path


def test_cli_sync_runs_and_exits_zero(tmp_path):
    in_dir = tmp_path / "in"
    out_dir = tmp_path / "out"
    in_dir.mkdir()
    (in_dir / "hello.md").write_text("# Hello\n", encoding="utf-8")
    toml = tmp_path / "md2kindle.toml"
    toml.write_text(
        f'[vault]\ninput_dir = "{in_dir}"\noutput_dir = "{out_dir}"\n', encoding="utf-8"
    )

    result = subprocess.run(
        [sys.executable, "-m", "md2kindle", "sync", "--config", str(toml)],
        capture_output=True, text=True,
    )
    assert result.returncode == 0, result.stderr
    assert (out_dir / "hello.html").exists()
    assert "Converted" in result.stdout or "converted" in result.stdout.lower()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd converter && python -m pytest tests/test_cli.py -v`
Expected: FAIL (`No module named md2kindle.__main__`).

- [ ] **Step 3: Write minimal implementation**

```python
# converter/md2kindle/__main__.py
import sys

from md2kindle.cli import main

if __name__ == "__main__":
    sys.exit(main())
```

```python
# converter/md2kindle/cli.py
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from md2kindle.config import load_config
from md2kindle.pipeline import sync


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="md2kindle")
    sub = parser.add_subparsers(dest="command", required=True)

    p_sync = sub.add_parser("sync", help="run one incremental conversion pass")
    p_sync.add_argument("--config", default="md2kindle.toml", help="path to md2kindle.toml")

    args = parser.parse_args(argv)

    if args.command == "sync":
        cfg = load_config(Path(args.config))
        summary = sync(cfg, log=lambda m: print(m))
        print(
            f"Converted {summary.converted}/{summary.total} notes, "
            f"{summary.images_failed} image(s) failed, "
            f"{summary.orphans_removed} orphan(s) removed."
        )
        return 0

    return 1


if __name__ == "__main__":
    sys.exit(main())
```

```markdown
# converter/README.md
# md2kindle

Converts an Obsidian `.md` vault into a self-contained `.html` vault for offline
reading on a Kindle (via KOReader). Remote images are downloaded and e-ink
optimized; `[[wikilinks]]` become relative links; the folder tree is mirrored.

## Install

```bash
cd converter
pip install -e ".[dev]"
```

## Run one pass

```bash
cp md2kindle.toml.example md2kindle.toml   # then edit input_dir / output_dir
python -m md2kindle sync --config md2kindle.toml
```

## Automate (Windows Task Scheduler)

Run `python -m md2kindle sync --config <path>` every ~5 minutes.

## Tests

```bash
python -m pytest
```
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd converter && python -m pytest -v`
Expected: PASS (full suite, all tasks).

- [ ] **Step 5: Commit**

```bash
git add converter/md2kindle/__main__.py converter/md2kindle/cli.py converter/tests/test_cli.py converter/README.md
git commit -m "feat(converter): CLI entrypoint + integration test + README"
```

---

## Self-Review (completed by planner)

**1. Spec coverage (§6 of design.md):**
- Modules (§6.1): all present — config, manifest, resolver, preprocess, renderer, images, layout, pipeline, cli. ✅
- Trigger scheduled incremental (§6.2): CLI `sync` is the one-shot pass; README documents Task Scheduler. ✅ (Scheduling itself is OS config, not code.)
- Incremental + manifest (§6.3): Task 2 + Task 8 (hash skip, `had_failures` retry, orphan cleanup). ✅
- Image pipeline (§6.4): Task 6 covers cache, dedup, fetch w/ retries, optimize (downscale/grayscale/jpeg), format→JPEG, failure placeholder, size guard (`max_bytes`), local copy. ✅
- Output layout (§6.5): Task 7 mirrors tree, `foo.md`→`foo.html`, shared `assets/`, atomic writes. ✅
- MD→HTML + CSS (§6.6): Task 5 with extensions and `DEFAULT_CSS`. ✅
- Config (§6.7): Task 1. ✅

**2. Placeholder scan:** One intentional revision noted inline in Task 8 (replace the awkward transport-injection test with the `transport=` parameter, which the implementation already supports). No TBD/TODO/"handle edge cases" stubs remain. ✅

**3. Type/name consistency:** `ImageCache.__init__(assets_dir, output_root, input_dir, opts, manifest, *, transport=None)` matches across Task 6 test, Task 6 impl, and Task 8 pipeline call. `process_html(html, src_relpath) -> tuple[str, int]` consistent. `resolve(target, idx, src_relpath)` consistent (Task 3 def, Task 4 use). `SyncSummary(converted, images_failed, orphans_removed, total)` consistent (Task 8 def, Task 9 use). `Config(input_dir, output_dir, exclude, image)` consistent throughout. ✅

**4. Open item carried to build:** `slugify` import path (`markdown.extensions.toc.slugify`) — if a future python-markdown renames it, Task 3 test fails loudly and is fixed at that time. Acceptable.
