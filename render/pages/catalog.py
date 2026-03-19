"""
render/pages/catalog.py — Render the tag catalog at /tags/.

Every named region across all watched repos, with:
  - prose excerpt from the leading comment block
  - source file and language
  - all prose documents that reference it
  - drift warnings at the top
"""
from __future__ import annotations

import sqlite3
from pathlib import Path
from typing import Any, Callable, Dict, List

LOG_NAME = "render.catalog"


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


def render_catalog(
    *,
    con: sqlite3.Connection,
    www: Path,
    env,
    site_cfg: Dict[str, Any],
    base_href_fn: Callable,
    canonical_fn: Callable,
    log,
) -> Dict[str, int]:
    import logging
    logger = logging.getLogger(LOG_NAME)

    # Tags with ref counts
    tags_raw = con.execute(
        """
        SELECT
            t.id, t.repo_id, t.name, t.source_file, t.language, t.prose,
            COUNT(r.id)                                      AS ref_count,
            SUM(CASE WHEN r.drifted = 1 THEN 1 ELSE 0 END)  AS drifted_count
        FROM tags t
        LEFT JOIN tag_refs r ON r.tag_id = t.id
        GROUP BY t.id
        ORDER BY t.repo_id, t.name COLLATE NOCASE
        """
    ).fetchall()

    # Drifted refs (no matching tag)
    orphans_raw = con.execute(
        """
        SELECT prose_file, name
        FROM tag_refs
        WHERE drifted = 1
        ORDER BY name COLLATE NOCASE
        """
    ).fetchall()

    tags: List[Dict[str, Any]] = []
    for row in tags_raw:
        refs = con.execute(
            "SELECT prose_file, drifted FROM tag_refs WHERE tag_id = ? ORDER BY prose_file",
            (row["id"],),
        ).fetchall()
        tags.append({
            "id":            row["id"],
            "repo_id":       row["repo_id"],
            "name":          row["name"],
            "source_file":   row["source_file"],
            "language":      row["language"] or "",
            "prose":         (row["prose"] or "").strip(),
            "ref_count":     row["ref_count"] or 0,
            "drifted_count": row["drifted_count"] or 0,
            "refs": [
                {
                    "prose_file": r["prose_file"],
                    "label":      Path(r["prose_file"]).name,
                    "drifted":    bool(r["drifted"]),
                }
                for r in refs
            ],
        })

    # Group by repo
    by_repo: Dict[str, List[Dict]] = {}
    for tag in tags:
        by_repo.setdefault(tag["repo_id"], []).append(tag)

    orphans = [
        {"name": r["name"], "label": Path(r["prose_file"]).name}
        for r in orphans_raw
    ]

    url      = "/tags/"
    out_path = www / "tags" / "index.html"

    try:
        tpl  = env.get_template("catalog.html.j2")
        html = tpl.render(
            tags=tags,
            by_repo=by_repo,
            orphans=orphans,
            total_tags=len(tags),
            total_refs=sum(t["ref_count"] for t in tags),
            total_drifted=sum(t["drifted_count"] for t in tags) + len(orphans),
            site=site_cfg,
            page={
                "title":     "Tag Catalog",
                "canonical": canonical_fn(url),
            },
            base_href=base_href_fn(out_path),
        )
        _write_if_changed(out_path, html.encode("utf-8"))
        logger.info("render.catalog: %d tags → %s", len(tags), out_path)
    except Exception as e:
        logger.warning("render.catalog: failed: %s", e)
        return {"tags": 0}

    return {
        "tags":    len(tags),
        "refs":    sum(t["ref_count"] for t in tags),
        "drifted": len(orphans),
    }
