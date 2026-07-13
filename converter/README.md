# md2kindle

Converts an Obsidian `.md` vault into a self-contained `.html` vault for offline
reading on a Kindle (via KOReader). Remote images are downloaded and e-ink
optimized; `[[wikilinks]]` become relative links; the folder tree is mirrored.

## Install

```bash
cd converter
pip install -e ".[dev]"
```

## Run one pass

```bash
cp md2kindle.toml.example md2kindle.toml   # then edit input_dir / output_dir
python -m md2kindle sync --config md2kindle.toml
```

> **Windows paths:** In `md2kindle.toml`, use **forward slashes** in paths (e.g. `C:/Users/me/vault`) — TOML treats backslashes as escape characters.

## Automate (Windows Task Scheduler)

Run `python -m md2kindle sync --config <path>` every ~5 minutes.

## Tests

```bash
python -m pytest
```
