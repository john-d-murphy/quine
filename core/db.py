"""
core/db.py — The quine database.

One SQLite file. Four tables:

  watches   — repos and directories being tracked
  items     — git commits and markdown files, anything with a date
  tags      — named @region markers found in source files
  tag_refs  — {@region:} directives found in prose, with drift detection

Tag identity is "{repo_id}/{name}" — collisions across repos are expected
and handled at the resolver level, not here.
"""
from __future__ import annotations

import json
import sqlite3
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, Generator, List, Optional


_DDL = """
CREATE TABLE IF NOT EXISTS watches (
    id            TEXT PRIMARY KEY,
    path          TEXT NOT NULL UNIQUE,
    name          TEXT,
    visibility    TEXT NOT NULL DEFAULT 'private',
    default_repo  INTEGER NOT NULL DEFAULT 0,  -- 1 = default for unscoped prose
    registered_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS items (
    id          TEXT PRIMARY KEY,
    source      TEXT NOT NULL,    -- 'git' | 'markdown'
    watch_id    TEXT,             -- which watch this belongs to
    ref         TEXT NOT NULL,    -- sha for git, relpath for markdown
    title       TEXT,
    body        TEXT,
    occurred_at TEXT,
    ingested_at TEXT NOT NULL,
    visibility  TEXT NOT NULL DEFAULT 'private',
    meta        TEXT              -- JSON blob
);

CREATE INDEX IF NOT EXISTS idx_items_source      ON items(source);
CREATE INDEX IF NOT EXISTS idx_items_watch_id    ON items(watch_id);
CREATE INDEX IF NOT EXISTS idx_items_occurred_at ON items(occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_items_visibility  ON items(visibility);

CREATE TABLE IF NOT EXISTS tags (
    id          TEXT PRIMARY KEY,   -- "{repo_id}/{name}"
    repo_id     TEXT NOT NULL,
    name        TEXT NOT NULL,
    source_file TEXT NOT NULL,      -- path relative to repo root
    language    TEXT,
    sha         TEXT,               -- sha1 of source file at ingest time
    prose       TEXT,               -- leading comment block, stripped
    ingested_at TEXT NOT NULL,
    UNIQUE(repo_id, name)
);

CREATE INDEX IF NOT EXISTS idx_tags_repo ON tags(repo_id);
CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);

CREATE TABLE IF NOT EXISTS tag_refs (
    id          TEXT PRIMARY KEY,   -- "{prose_file}#{name}"
    tag_id      TEXT,               -- NULL if drifted
    prose_file  TEXT NOT NULL,
    name        TEXT NOT NULL,      -- bare name as written in directive
    repo_id     TEXT,               -- resolved repo_id (NULL if drifted)
    drifted     INTEGER NOT NULL DEFAULT 0,
    ingested_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tag_refs_tag_id     ON tag_refs(tag_id);
CREATE INDEX IF NOT EXISTS idx_tag_refs_prose_file ON tag_refs(prose_file);
CREATE INDEX IF NOT EXISTS idx_tag_refs_drifted    ON tag_refs(drifted);
"""


def open_db(path: Path) -> sqlite3.Connection:
    path.parent.mkdir(parents=True, exist_ok=True)
    con = sqlite3.connect(str(path))
    con.row_factory = sqlite3.Row
    con.execute("PRAGMA journal_mode=WAL")
    con.execute("PRAGMA foreign_keys=ON")
    con.executescript(_DDL)
    con.commit()
    return con


@contextmanager
def db_conn(path: Path) -> Generator[sqlite3.Connection, None, None]:
    con = open_db(path)
    try:
        yield con
        con.commit()
    except Exception:
        con.rollback()
        raise
    finally:
        con.close()


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


# ── watches ────────────────────────────────────────────────────────

def upsert_watch(
    con: sqlite3.Connection,
    *,
    id: str,
    path: str,
    name: Optional[str] = None,
    visibility: str = "private",
    default_repo: bool = False,
) -> None:
    con.execute(
        """
        INSERT INTO watches (id, path, name, visibility, default_repo, registered_at)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            path         = excluded.path,
            name         = excluded.name,
            visibility   = excluded.visibility,
            default_repo = excluded.default_repo
        """,
        (id, path, name, visibility, int(default_repo), now_iso()),
    )


def all_watches(con: sqlite3.Connection) -> List[sqlite3.Row]:
    return con.execute("SELECT * FROM watches ORDER BY name").fetchall()


def default_watch(con: sqlite3.Connection) -> Optional[sqlite3.Row]:
    return con.execute(
        "SELECT * FROM watches WHERE default_repo = 1 LIMIT 1"
    ).fetchone()


# ── items ──────────────────────────────────────────────────────────

def upsert_item(
    con: sqlite3.Connection,
    *,
    id: str,
    source: str,
    watch_id: Optional[str] = None,
    ref: str,
    title: Optional[str] = None,
    body: Optional[str] = None,
    occurred_at: Optional[str] = None,
    visibility: str = "private",
    meta: Optional[Dict[str, Any]] = None,
) -> None:
    meta_json = json.dumps(meta, ensure_ascii=False) if meta else None
    con.execute(
        """
        INSERT INTO items
            (id, source, watch_id, ref, title, body, occurred_at, ingested_at, visibility, meta)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            title       = excluded.title,
            body        = excluded.body,
            occurred_at = excluded.occurred_at,
            ingested_at = excluded.ingested_at,
            meta        = excluded.meta,
            visibility  = CASE
                WHEN excluded.visibility = 'public'  THEN 'public'
                WHEN excluded.visibility = 'unlisted'
                     AND items.visibility = 'private' THEN 'unlisted'
                ELSE items.visibility
            END
        """,
        (id, source, watch_id, ref, title, body, occurred_at, now_iso(), visibility, meta_json),
    )


def item_meta(row: sqlite3.Row) -> Dict[str, Any]:
    try:
        return json.loads(row["meta"] or "{}")
    except Exception:
        return {}


# ── tags ───────────────────────────────────────────────────────────

def upsert_tag(
    con: sqlite3.Connection,
    *,
    repo_id: str,
    name: str,
    source_file: str,
    language: Optional[str] = None,
    sha: Optional[str] = None,
    prose: Optional[str] = None,
) -> None:
    tag_id = f"{repo_id}/{name}"
    con.execute(
        """
        INSERT INTO tags (id, repo_id, name, source_file, language, sha, prose, ingested_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(repo_id, name) DO UPDATE SET
            source_file = excluded.source_file,
            language    = excluded.language,
            sha         = excluded.sha,
            prose       = excluded.prose,
            ingested_at = excluded.ingested_at
        """,
        (tag_id, repo_id, name, source_file, language, sha, prose, now_iso()),
    )


def resolve_tag(
    con: sqlite3.Connection,
    name: str,
    *,
    prose_file: Optional[Path] = None,
    default_repo_id: Optional[str] = None,
) -> Optional[sqlite3.Row]:
    """
    Resolve a bare tag name to a tag row.

    Resolution order:
      1. Exact qualified name "repo_id/name" — always unambiguous
      2. Proximity — tag in same repo as prose_file (by path prefix)
      3. Default repo — watches.default_repo = 1
      4. Unambiguous singleton — only one tag with that name across all repos
      5. Ambiguous — return None, caller should warn

    For qualified names (containing '/'), steps 2-5 are skipped.
    """
    # Qualified: "repo_id/name"
    if "/" in name:
        return con.execute(
            "SELECT * FROM tags WHERE id = ?", (name,)
        ).fetchone()

    candidates = con.execute(
        "SELECT * FROM tags WHERE name = ?", (name,)
    ).fetchall()

    if not candidates:
        return None

    if len(candidates) == 1:
        return candidates[0]

    # Proximity: find which repo the prose file lives under
    if prose_file:
        prose_str = str(prose_file.resolve())
        for row in con.execute("SELECT id, path FROM watches").fetchall():
            if prose_str.startswith(row["path"]):
                match = next((c for c in candidates if c["repo_id"] == row["id"]), None)
                if match:
                    return match

    # Default repo
    if default_repo_id:
        match = next((c for c in candidates if c["repo_id"] == default_repo_id), None)
        if match:
            return match

    # Ambiguous
    return None


# ── tag_refs ───────────────────────────────────────────────────────

def upsert_tag_ref(
    con: sqlite3.Connection,
    *,
    prose_file: str,
    name: str,
    tag_id: Optional[str],
    repo_id: Optional[str],
    drifted: bool = False,
) -> None:
    ref_id = f"{prose_file}#{name}"
    con.execute(
        """
        INSERT INTO tag_refs (id, tag_id, prose_file, name, repo_id, drifted, ingested_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            tag_id      = excluded.tag_id,
            repo_id     = excluded.repo_id,
            drifted     = excluded.drifted,
            ingested_at = excluded.ingested_at
        """,
        (ref_id, tag_id, prose_file, name, repo_id, int(drifted), now_iso()),
    )


def all_tags(con: sqlite3.Connection) -> List[sqlite3.Row]:
    return con.execute(
        "SELECT * FROM tags ORDER BY repo_id, name COLLATE NOCASE"
    ).fetchall()


def tags_for_repo(con: sqlite3.Connection, repo_id: str) -> List[sqlite3.Row]:
    return con.execute(
        "SELECT * FROM tags WHERE repo_id = ? ORDER BY name COLLATE NOCASE",
        (repo_id,),
    ).fetchall()


def refs_for_tag(con: sqlite3.Connection, tag_id: str) -> List[sqlite3.Row]:
    return con.execute(
        "SELECT * FROM tag_refs WHERE tag_id = ? ORDER BY prose_file",
        (tag_id,),
    ).fetchall()


def drifted_refs(con: sqlite3.Connection) -> List[sqlite3.Row]:
    return con.execute(
        "SELECT * FROM tag_refs WHERE drifted = 1 ORDER BY prose_file, name"
    ).fetchall()
