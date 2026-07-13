from pathlib import Path
from md2kindle.config import Config, ImageOptions, load_config, DEFAULT_EXCLUDE


def test_load_config_parses_required_and_defaults(tmp_path):
    toml = tmp_path / "md2kindle.toml"
    toml.write_text(
        '[vault]\ninput_dir = "vault"\noutput_dir = "vault.kindle"\n',
        encoding="utf-8",
    )
    cfg = load_config(toml)
    assert isinstance(cfg, Config)
    assert cfg.input_dir == Path("vault")
    assert cfg.output_dir == Path("vault.kindle")
    assert cfg.exclude == DEFAULT_EXCLUDE
    assert isinstance(cfg.image, ImageOptions)
    assert cfg.image.max_width == 1000
    assert cfg.image.grayscale is True


def test_load_config_overrides_image_options(tmp_path):
    toml = tmp_path / "c.toml"
    toml.write_text(
        '[vault]\ninput_dir = "in"\noutput_dir = "out"\n'
        '[image]\nmax_width = 600\ngrayscale = false\n',
        encoding="utf-8",
    )
    cfg = load_config(toml)
    assert cfg.image.max_width == 600
    assert cfg.image.grayscale is False
