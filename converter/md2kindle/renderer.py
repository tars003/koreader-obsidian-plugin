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
