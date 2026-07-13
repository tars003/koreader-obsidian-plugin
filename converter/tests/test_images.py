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


def test_remote_non_image_bytes_fail_soft(tmp_path):
    # soft-404 / non-image payload: HTTP 200 with HTML bytes that cannot be decoded
    transport = httpx.MockTransport(
        lambda req: httpx.Response(200, content=b"<html>not an image</html>\n")
    )
    cache, _, m = _cache(tmp_path, transport=transport)
    # must not raise
    html, failed = cache.process_html('<img src="https://x/a.png">', "note.md")
    assert failed == 1
    assert "[image unavailable: https://x/a.png]" in html
    assert m.get_image("https://x/a.png")["failed"] is True


def test_corrupt_image_bytes_fail_soft(tmp_path):
    # download succeeds (HTTP 200) but bytes are a truncated/corrupt PNG
    transport = httpx.MockTransport(
        lambda req: httpx.Response(200, content=b"\x89PNG\r\n\x1a\n")
    )
    cache, _, m = _cache(tmp_path, transport=transport)
    # must not raise
    html, failed = cache.process_html('<img src="https://x/a.png">', "note.md")
    assert failed == 1
    assert "[image unavailable: https://x/a.png]" in html
    assert m.get_image("https://x/a.png")["failed"] is True


def test_transparent_png_composites_to_white(tmp_path):
    # a fully-transparent RGBA PNG: transparent areas must composite onto white
    # (not become black) before grayscale conversion
    cache, in_root, _ = _cache(tmp_path)
    buf = io.BytesIO()
    Image.new("RGBA", (50, 50), (0, 0, 0, 0)).save(buf, format="PNG")
    (in_root / "trans.png").write_bytes(buf.getvalue())

    html, failed = cache.process_html('<img src="trans.png">', "note.md")
    assert failed == 0
    files = list((tmp_path / "assets").glob("*.jpg"))
    assert len(files) == 1
    im = Image.open(files[0])
    assert im.mode == "L"  # grayscale (cfg has grayscale=True)
    assert im.getpixel((10, 10)) == 255  # transparent area -> white, not black
