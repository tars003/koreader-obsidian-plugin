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
    # Strip HTML comments first so wikilinks inside them are never rewritten
    # (single- and multi-line comments are both covered by DOTALL).
    md = re.sub(r"<!--.*?-->", "", md, flags=re.DOTALL)
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


_TAG_RE = re.compile(r'(?<!\S)#([\w/-]+)')
_HIGHLIGHT_RE = re.compile(r'==(?:\{([^}]*)\})?(.+?)==')


def _rewrite_tags(text: str) -> str:
    """Wrap #tag in a styled span."""
    return _TAG_RE.sub(r'<span class="tag">#\1</span>', text)


def _rewrite_highlights(text: str) -> str:
    """Convert ==text== (with optional {color}) to <mark>."""
    return _HIGHLIGHT_RE.sub(r'<mark>\2</mark>', text)


def _rewrite_text(text: str, src_relpath: str, idx: NoteIndex, log) -> str:
    text = _WIKI_RE.sub(lambda m: _handle(m, src_relpath, idx, log), text)
    text = _MD_LINK_RE.sub(_rewrite_md_link, text)
    text = _rewrite_tags(text)
    text = _rewrite_highlights(text)
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
