from __future__ import annotations

import re
from html.parser import HTMLParser

import markdown

DEFAULT_CSS = """
body { font-family: serif; line-height: 1.5; margin: 4%; color: black; background: white; }
h1 { font-size: 1.4em; margin-top: 0; border-bottom: 1px solid #888; padding-bottom: 0.2em; }
h2, h3, h4, h5 { line-height: 1.25; }
h6 { font-size: 1em; font-weight: normal; margin: 0; display: list-item; }
img { max-width: 100%; height: auto; }
code, pre { font-family: monospace; }
pre { background: #eee; padding: 0.5em; white-space: pre-wrap; }
.broken-link { color: #888; }
ul, ol { margin: 0.3em 0; padding-left: 1.5em; }
li { margin: 0.1em 0; }
p { margin: 0.4em 0; }
.tag { display: inline-block; background: #ddd; border: 1px solid #bbb; border-radius: 3px; padding: 1px 5px; font-size: 0.85em; margin: 0 1px; }
mark { background-color: #666; color: white; padding: 0 2px; }
.toc { margin: 1em 0; padding: 0.5em; border: 1px solid #ccc; }
.toc ul { list-style: none; padding-left: 0.8em; margin: 0.1em 0; }
.toc li { margin: 0.15em 0; }
.toc-li { font-size: 0.9em; }
.toc-links { border-left: 2px solid #ddd; margin: 0.1em 0 0.3em 0; }
.toctitle { font-weight: bold; display: block; margin-bottom: 0.3em; }
.note-title { font-size: 1.5em; font-weight: bold; margin-top: 0; margin-bottom: 0.5em; border-bottom: 1px solid #888; padding-bottom: 0.2em; }
.note-title.repeat { font-size: 1.2em; page-break-before: always; }
.page-break { page-break-after: always; }
""".strip()

_EXTENSIONS = ["tables", "fenced_code", "attr_list", "toc", "admonition"]


def _has_h1(md: str) -> bool:
    """Check if the markdown starts with a level-1 heading."""
    for line in md.splitlines():
        s = line.strip()
        if s.startswith("# ") and not s.startswith("## "):
            return True
    return False


def _count_headings(html: str) -> int:
    """Count <h1>–<h6> elements in rendered HTML."""
    return len(re.findall(r"<h[1-6][ >]", html))


# ---- nested list-item parser for TOC enrichment ----


class _ListDepthParser(HTMLParser):
    """Walk HTML and record (depth, text) for significant <li> elements."""

    def __init__(self) -> None:
        super().__init__()
        self.results: list[tuple[int, str]] = []
        self._depth = 0
        self._in_li = False
        self._li_text = ""
        self._li_sig = False

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag in ("ol", "ul"):
            self._depth += 1
        elif tag == "li":
            self._in_li = True
            self._li_text = ""
            self._li_sig = False
        elif tag in ("a", "strong") and self._in_li:
            self._li_sig = True
        elif tag == "span" and self._in_li:
            a = dict(attrs)
            if "tag" in (a.get("class") or ""):
                self._li_sig = True

    def handle_endtag(self, tag: str) -> None:
        if tag in ("ol", "ul"):
            self._depth -= 1
        elif tag == "li":
            self._in_li = False
            text = re.sub(r"<[^>]+>", "", self._li_text).strip()[:80]
            if self._li_sig and text:
                self.results.append((self._depth, text))

    def handle_data(self, data: str) -> None:
        if self._in_li:
            self._li_text += data


def _build_nested_lis(results: list[tuple[int, str]]) -> str:
    """Given (depth, text) pairs, return nested <ul> HTML."""
    if not results:
        return ""
    parts: list[str] = []
    prev = 0
    for depth, text in results:
        while depth > prev:
            parts.append('<ul class="toc-links">')
            prev += 1
        while depth < prev:
            parts.append("</ul>")
            prev -= 1
        parts.append(f'<li class="toc-li">· {text}</li>')
    while prev > 0:
        parts.append("</ul>")
        prev -= 1
    return "".join(parts)


def _enrich_toc(toc_html: str, body: str) -> str:
    """Add nested list-items (with links/tags/bold) to the heading-based TOC."""
    if not toc_html:
        return toc_html

    # Find heading positions in the body
    heading_starts = [
        (m.start(), m.group(2))
        for m in re.finditer(r'<h([1-6])\s[^>]*id="([^"]*)"[^>]*>', body)
    ]
    if not heading_starts:
        return toc_html

    # For each heading section, parse nested <li> structure
    section_lis: dict[str, str] = {}  # anchor → nested-ul HTML string
    for i, (start, anchor) in enumerate(heading_starts):
        if not anchor:
            continue
        end = heading_starts[i + 1][0] if i + 1 < len(heading_starts) else len(body)
        parser = _ListDepthParser()
        parser.feed(body[start:end])
        if parser.results:
            section_lis[anchor] = _build_nested_lis(parser.results)

    if not section_lis:
        return toc_html

    # Insert list-item HTML into the TOC under each heading's anchor
    for anchor, li_html in section_lis.items():
        toc_html = re.sub(
            rf'(<a href="#{re.escape(anchor)}">[^<]*</a>)',
            lambda m, _li=li_html: m.group(0) + _li,
            toc_html,
        )

    return toc_html


# ---- main rendering ----


def render_markdown(md: str, css: str = DEFAULT_CSS, title: str | None = None) -> str:
    """Convert markdown to a self-contained HTML document.

    * If *title* is given and the markdown has no H1, inject a styled ``<div>``
      title at the top of the body (visible but **not** a heading — preserves
      heading levels for TOC nesting).
    * If the rendered document has 2+ headings, inject a Table of Contents
      (generated by the ``toc`` extension) after the first H1.
    """
    # Use the class-based API so we can access md.toc
    md_inst = markdown.Markdown(extensions=_EXTENSIONS)
    body = md_inst.convert(md)
    toc_html = getattr(md_inst, "toc", "")

    # Visible title without consuming a heading level
    if title and not _has_h1(md):
        body = f'<div class="note-title">{title}</div>\n{body}'

    heading_count = _count_headings(body)
    try:
        toc_html = _enrich_toc(toc_html, body)
    except Exception:
        pass  # enrichment failed — use heading-only TOC

    if toc_html and heading_count >= 2:
        toc_html = toc_html.replace(
            '<span class="toctitle">Table of Contents</span>',
            '<span class="toctitle">Contents</span>',
        )
        # Inject TOC after the first H1, or right after the title div
        m = re.search(r"</h1>", body)
        if m:
            insert_at = m.end()
        else:
            insert_at = 0
            if title and ("class=\"note-title\"" in body[:200]):
                dm = re.search(r"</div>", body)
                if dm:
                    insert_at = dm.end()  # right after the title div
        # Build TOC block: TOC + optional page-break + repeated title
        toc_block = toc_html
        if title:
            toc_block += '\n<div class="page-break"></div>\n'
            toc_block += f'<div class="note-title repeat">{title}</div>\n'
        body = body[:insert_at] + "\n" + toc_block + body[insert_at:]

    return (
        "<!DOCTYPE html>\n"
        '<html lang="en">\n<head>\n<meta charset="utf-8">\n'
        f"<style>\n{css}\n</style>\n</head>\n<body>\n{body}\n</body>\n</html>\n"
    )
