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
