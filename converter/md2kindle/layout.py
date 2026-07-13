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
