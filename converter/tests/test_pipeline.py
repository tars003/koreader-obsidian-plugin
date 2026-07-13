from md2kindle.config import Config, ImageOptions
from md2kindle.manifest import Manifest
from md2kindle.pipeline import sync


def _cfg(tmp_path):
    in_dir = tmp_path / "in"
    out_dir = tmp_path / "out"
    in_dir.mkdir()
    cfg = Config(input_dir=in_dir, output_dir=out_dir)
    cfg.image = ImageOptions(max_width=400, grayscale=True, retries=0)
    return cfg, in_dir, out_dir


def test_sync_converts_markdown_and_rewrites_wikilink(tmp_path, fake_remote_transport):
    cfg, in_dir, out_dir = _cfg(tmp_path)
    (in_dir / "a.md").write_text("# A\nLink to [[b]] and ![img](https://x/i.png)\n", encoding="utf-8")
    (in_dir / "b.md").write_text("# B\n", encoding="utf-8")
    summary = sync(cfg, log=lambda *_: None, transport=fake_remote_transport)
    assert summary.total == 2 and summary.converted == 2
    a = (out_dir / "a.html").read_text(encoding="utf-8")
    assert 'href="b.html"' in a
    assert "assets/" in a
    assert "[[b]]" not in a


def test_sync_is_idempotent_second_run_no_conversions(tmp_path):
    cfg, in_dir, out_dir = _cfg(tmp_path)
    (in_dir / "a.md").write_text("# A\n", encoding="utf-8")
    sync(cfg, log=lambda *_: None)
    s2 = sync(cfg, log=lambda *_: None)
    assert s2.converted == 0


def test_sync_removes_orphan_output(tmp_path):
    cfg, in_dir, out_dir = _cfg(tmp_path)
    (in_dir / "a.md").write_text("# A\n", encoding="utf-8")
    sync(cfg, log=lambda *_: None)
    (in_dir / "a.md").unlink()
    s = sync(cfg, log=lambda *_: None)
    assert s.orphans_removed == 1
    assert not (out_dir / "a.html").exists()


def test_sync_excludes_obsidian_folder(tmp_path):
    cfg, in_dir, out_dir = _cfg(tmp_path)
    (in_dir / ".obsidian").mkdir()
    (in_dir / ".obsidian" / "app.json").write_text("{}", encoding="utf-8")
    (in_dir / "keep.md").write_text("# K\n", encoding="utf-8")
    s = sync(cfg, log=lambda *_: None)
    assert s.total == 1
    assert not (out_dir / ".obsidian").exists()


def test_noop_pass_does_not_save_manifest(tmp_path, monkeypatch):
    cfg, in_dir, out_dir = _cfg(tmp_path)
    (in_dir / "a.md").write_text("# A\n", encoding="utf-8")
    # first pass: performs writes -> manifest.save() is invoked
    sync(cfg, log=lambda *_: None)

    calls = {"count": 0}

    def counting_save(self):
        calls["count"] += 1

    monkeypatch.setattr(Manifest, "save", counting_save)
    # second pass: nothing changed -> no source converted, no orphan removed
    sync(cfg, log=lambda *_: None)
    assert calls["count"] == 0
