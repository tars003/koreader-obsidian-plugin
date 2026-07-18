from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

try:  # Python >= 3.11
    import tomllib
except ModuleNotFoundError:  # Python 3.10
    import tomli as tomllib  # type: ignore[no-redef]

DEFAULT_EXCLUDE: list[str] = [".obsidian/*", "templates/*"]


@dataclass
class ImageOptions:
    max_width: int = 1000
    grayscale: bool = True
    jpeg_quality: int = 70
    timeout: float = 15.0
    retries: int = 1
    max_bytes: int = 8 * 1024 * 1024  # 8 MB


@dataclass
class Config:
    input_dir: Path
    output_dir: Path
    exclude: list[str] = field(default_factory=lambda: list(DEFAULT_EXCLUDE))
    image: ImageOptions = field(default_factory=ImageOptions)


def load_config(path: Path) -> Config:
    with open(path, "rb") as f:
        data = tomllib.load(f)
    vault = data["vault"]
    cfg = Config(
        input_dir=Path(vault["input_dir"]),
        output_dir=Path(vault["output_dir"]),
        exclude=list(data.get("exclude", DEFAULT_EXCLUDE)),
    )
    img = data.get("image", {})
    if img:
        cfg.image = ImageOptions(**{k: img[k] for k in img})
    return cfg
