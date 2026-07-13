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
        f'[vault]\ninput_dir = "{in_dir.as_posix()}"\noutput_dir = "{out_dir.as_posix()}"\n', encoding="utf-8"
    )

    result = subprocess.run(
        [sys.executable, "-m", "md2kindle", "sync", "--config", str(toml)],
        capture_output=True, text=True,
    )
    assert result.returncode == 0, result.stderr
    assert (out_dir / "hello.html").exists()
    assert "Converted" in result.stdout or "converted" in result.stdout.lower()
