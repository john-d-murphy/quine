#!/usr/bin/env python3
"""
quine — collect → render pipeline.

Usage:
    python quine.py --action collect --config config.yml
    python quine.py --action render  --config config.yml
"""
import argparse
import logging
from core.db import open_db, upsert_watch
from collect.git import ingest_repo
from collect.tags import collect_tags

import sys
from pathlib import Path

import yaml

LOG_FORMAT = "[%(asctime)s  %(name)-20s] %(message)s"
logging.basicConfig(level=logging.INFO, format=LOG_FORMAT)
log = logging.getLogger("quine")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="quine — static site pipeline"
    )
    parser.add_argument("--action", required=True, choices=list(ACTIONS))
    parser.add_argument("--config", default="config.yml")
    args = parser.parse_args()

    cfg = _load_config(args.config)
    log.info("quine: action=%s config=%s", args.action, args.config)
    return ACTIONS[args.action](cfg)


def _load_config(path: str) -> dict:
    with open(path, encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


def action_collect(cfg: dict) -> int:
    sys_cfg = cfg.get("system", {})
    db_path = Path(sys_cfg.get("db", "quine.db"))
    watches_cfg = cfg.get("watches", [])

    con = open_db(db_path)
    for w in watches_cfg:
        upsert_watch(
            con,
            id=w["id"],
            path=str(Path(w["path"]).resolve()),
            name=w.get("name", w["id"]),
            visibility=w.get("visibility", "private"),
            default_repo=bool(w.get("default", False)),
        )
    con.commit()

    for w in watches_cfg:
        ingest_repo(
            con,
            watch_id=w["id"],
            repo_path=Path(w["path"]).resolve(),
            visibility=w.get("visibility", "private"),
        )
    con.commit()
    con.close()

    watches_rows = [
        {"id": w["id"], "path": str(Path(w["path"]).resolve())}
        for w in watches_cfg
    ]
    prose_roots = [
        Path(p).resolve()
        for p in cfg.get("prose_roots", [w["path"] for w in watches_cfg])
    ]
    default_repo = cfg.get("site", {}).get("default_repo", "")

    report = collect_tags(
        db_path=db_path,
        watches=watches_rows,
        prose_roots=prose_roots,
        default_repo_id=default_repo or None,
    )
    log.info("collect: tags → %s", report)
    return 0


def action_render(cfg: dict) -> int:
    from render.site import run

    return run(cfg)


ACTIONS = {
    "collect": action_collect,
    "render": action_render,
}


if __name__ == "__main__":
    sys.exit(main())
