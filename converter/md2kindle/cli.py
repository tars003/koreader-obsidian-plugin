from __future__ import annotations

import argparse
import sys
from pathlib import Path

from md2kindle.config import load_config
from md2kindle.pipeline import sync


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="md2kindle")
    sub = parser.add_subparsers(dest="command", required=True)

    p_sync = sub.add_parser("sync", help="run one incremental conversion pass")
    p_sync.add_argument("--config", default="md2kindle.toml", help="path to md2kindle.toml")

    args = parser.parse_args(argv)

    if args.command == "sync":
        cfg = load_config(Path(args.config))
        summary = sync(cfg, log=lambda m: print(m))
        print(
            f"Converted {summary.converted}/{summary.total} notes, "
            f"{summary.images_failed} image(s) failed, "
            f"{summary.orphans_removed} orphan(s) removed."
        )
        return 0

    return 1


if __name__ == "__main__":
    sys.exit(main())
