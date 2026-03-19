"""
render/site.py — Render all pages from the quine DB.

Reads the DB, emits HTML into www/. One pass, no cache.
"""
from __future__ import annotations

import logging
import shutil
from pathlib import Path
from typing import Any, Callable, Dict, Optional

LOG = logging.getLogger("render")


def _md_converter():
    try:
        import markdown
        def convert(text: str) -> str:
            md = markdown.Markdown(
                extensions=["extra", "smarty", "fenced_code", "attr_list", "codehilite"]
            )
            return md.convert(text)
        return convert
    except ImportError:
        def convert(text: str) -> str:
            return f"<pre>{text}</pre>"
        return convert


def _base_href(www: Path, out_path: Path, base_url: str) -> str:
    if base_url:
        return base_url.rstrip("/") + "/"
    try:
        depth = len(out_path.relative_to(www).parts) - 1
        return ("../" * depth) or "./"
    except ValueError:
        return "./"


def _canonical(base_url: str, url: str) -> str:
    if not base_url:
        return url
    return base_url.rstrip("/") + url


def run(cfg: dict) -> int:
    from jinja2 import Environment, FileSystemLoader, select_autoescape
    from core.db import open_db, all_watches, drifted_refs

    sys_cfg       = cfg.get("system", {})
    db_path       = Path(sys_cfg.get("db", "quine.db"))
    www           = Path(sys_cfg.get("www", "www"))
    templates_dir = Path(sys_cfg.get("templates", "templates"))
    static_dir    = Path(sys_cfg.get("static", "static"))
    base_url      = cfg.get("site", {}).get("base_url", "")
    target        = cfg.get("render", {}).get("target", "public")
    default_repo  = cfg.get("site", {}).get("default_repo", "")

    www.mkdir(parents=True, exist_ok=True)

    # Copy static assets
    if static_dir.exists():
        dst_assets = www / "assets"
        dst_assets.mkdir(parents=True, exist_ok=True)
        for f in static_dir.rglob("*"):
            if f.is_file():
                dst = dst_assets / f.relative_to(static_dir)
                dst.parent.mkdir(parents=True, exist_ok=True)
                if not dst.exists() or f.read_bytes() != dst.read_bytes():
                    shutil.copy2(f, dst)

    env = Environment(
        loader=FileSystemLoader(str(templates_dir)),
        autoescape=select_autoescape(["html"]),
    )

    site_cfg = cfg.get("site", {})
    md_fn    = _md_converter()
    con      = open_db(db_path)

    base_href_fn = lambda out: _base_href(www, out, base_url)
    canonical_fn = lambda url: _canonical(base_url, url)

    # Visibility filter
    vis = ("public",) if target == "public" else ("public", "unlisted", "private")

    # ── Project pages ──────────────────────────────────────────────
    from render.pages.project import render_project

    watches = all_watches(con)
    projects_written = 0
    for watch in watches:
        if watch["visibility"] not in vis:
            continue
        ok = render_project(
            watch=watch,
            con=con,
            www=www,
            env=env,
            site_cfg=site_cfg,
            base_href_fn=base_href_fn,
            canonical_fn=canonical_fn,
            md_fn=md_fn,
            default_repo_id=default_repo or None,
            log=LOG,
        )
        if ok:
            projects_written += 1

    LOG.info("render: %d project pages", projects_written)

    # ── Tag catalog ────────────────────────────────────────────────
    from render.pages.catalog import render_catalog

    report = render_catalog(
        con=con,
        www=www,
        env=env,
        site_cfg=site_cfg,
        base_href_fn=base_href_fn,
        canonical_fn=canonical_fn,
        log=LOG,
    )
    LOG.info("render: catalog → %s", report)

    # ── Drift report ───────────────────────────────────────────────
    drifted = drifted_refs(con)
    if drifted:
        LOG.warning("render: %d drifted tag references", len(drifted))
        for row in drifted:
            LOG.warning("  {@region: %s} in %s", row["name"], Path(row["prose_file"]).name)

    con.close()
    LOG.info("render: done → %s", www)
    return 0
