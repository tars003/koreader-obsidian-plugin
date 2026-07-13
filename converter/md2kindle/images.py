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
    if im.mode in ("RGBA", "LA") or (im.mode == "P" and "transparency" in im.info):
        bg = Image.new("RGBA", im.size, (255, 255, 255, 255))
        bg.alpha_composite(im.convert("RGBA"))
        im = bg.convert("RGB")
    elif im.mode != "L":
        im = im.convert("RGB")
    if opts.grayscale:
        im = im.convert("L")
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
            print(f"  ↡ {src}", flush=True)
            data, _err = _fetch_remote(src, self.opts, self.transport)
            if data is None:
                self.manifest.set_image(src, None, None, True)
                return None
            try:
                fname = self._store(data)
            except Exception:
                self.manifest.set_image(src, None, None, True)
                return None
            self.manifest.set_image(src, hash_bytes(data), f"assets/{fname}", False)
            return fname
        # local attachment, relative to the note's directory in the input vault
        local = self.input_dir / _posix_dir(src_relpath) / src
        if not local.exists():
            return None
        try:
            return self._store(local.read_bytes())
        except Exception:
            return None
