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
