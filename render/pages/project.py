"""
render/pages/project.py — Render a project page at /project/{id}/.

Shows life.md with {@region:} directives expanded, followed by the
recent commit log. Source files referenced by directives are copied
to www/source/{repo_id}/{relpath} and download links point there.
"""
from __future__ import annotations

import re
import shutil
import sqlite3
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional

LOG_NAME = "render.project"


def _format_date(iso: Optional[str]) -> str:
    return (iso or "")[:10]


def _write_if_changed(path: Path, data: bytes) -> bool:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        try:
            if path.read_bytes() == data:
                return False
        except Exception:
            pass
    path.write_bytes(data)
    return True


def _copy_source_files(html: str, *, repo_root: Path, repo_id: str, www: Path) -> None:
    """
    The directives renderer emits /source/{repo_id}/{relpath} URLs.
    Copy the actual files into www/source/{repo_id}/.
    """
    for rel in re.findall(rf"/source/{re.escape(repo_id)}/([^'\"]+)", html):
        src = repo_root / rel
        dst = www / "source" / repo_id / rel
        if src.exists() and not dst.exists():
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)


def render_project(
    *,
    watch: sqlite3.Row,
    con: sqlite3.Connection,
    www: Path,
    env,
    site_cfg: Dict[str, Any],
    base_href_fn: Callable,
    canonical_fn: Callable,
    md_fn: Callable[[str], str],
    default_repo_id: Optional[str] = None,
    log,
) -> bool:
    """Render one project page. Returns True if written."""
    import logging
    logger = logging.getLogger(LOG_NAME)

    watch_id   = watch["id"]
    watch_name = watch["name"] or watch_id
    repo_root  = Path(watch["path"])

    # ── Render life.md ─────────────────────────────────────────────
    life_md_path = repo_root / "life.md"
    life_html = ""

    if life_md_path.exists():
        try:
            import yaml
            from core.directives import process_directives, parse_front_matter_annotates

            raw = life_md_path.read_text(encoding="utf-8", errors="replace")

            # Strip front matter
            fm: dict = {}
            body = raw
            fm_m = re.match(r"^(?:\ufeff)?---\s*\n(.*?)\n---\s*\n", raw, re.DOTALL)
            if fm_m:
                try:
                    fm = yaml.safe_load(fm_m.group(1)) or {}
                except Exception:
                    pass
                body = raw[fm_m.end():]

            annotates = parse_front_matter_annotates(fm, md_path=life_md_path)

            body = process_directives(
                body,
                md_path=life_md_path,
                con=con,
                annotates=annotates,
                default_repo_id=default_repo_id,
            )
            life_html = md_fn(body)
            _copy_source_files(life_html, repo_root=repo_root, repo_id=watch_id, www=www)

        except Exception as e:
            logger.warning("render.project: life.md failed for %s: %s", watch_id, e)

    # ── Commits ────────────────────────────────────────────────────
    rows = con.execute(
        """
        SELECT title, occurred_at, meta
        FROM items
        WHERE source = 'git' AND watch_id = ?
        ORDER BY occurred_at DESC
        LIMIT 30
        """,
        (watch_id,),
    ).fetchall()

    import json
    commits: List[Dict[str, str]] = []
    for row in rows:
        meta: dict = {}
        try:
            meta = json.loads(row["meta"] or "{}")
        except Exception:
            pass
        commits.append({
            "sha_short":   meta.get("sha_short", ""),
            "title":       row["title"] or "",
            "author":      meta.get("author", ""),
            "date":        _format_date(row["occurred_at"]),
        })

    # ── Render ─────────────────────────────────────────────────────
    url      = f"/project/{watch_id}/"
    out_path = www / "project" / watch_id / "index.html"

    try:
        tpl  = env.get_template("project.html.j2")
        html = tpl.render(
            watch={
                "id":        watch_id,
                "name":      watch_name,
                "life_html": life_html,
                "url":       url,
            },
            commits=commits,
            site=site_cfg,
            page={
                "title":     watch_name,
                "canonical": canonical_fn(url),
            },
            base_href=base_href_fn(out_path),
        )
        _write_if_changed(out_path, html.encode("utf-8"))
        logger.info("render.project: %s → %s", watch_name, out_path)
        return True
    except Exception as e:
        logger.warning("render.project: failed %s: %s", watch_id, e)
        return False
