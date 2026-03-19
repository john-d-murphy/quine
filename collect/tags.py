"""
collect/tags.py — Build the tag registry.

Two passes:

1. SOURCE PASS — walk every watched repo, extract @region markers,
   upsert into tags. Skips hidden dirs, node_modules, build artifacts.

2. PROSE PASS — scan markdown files for {@region:} directives,
   resolve each against the tag registry using proximity/default/singleton
   logic, upsert into tag_refs. Marks unresolvable refs as drifted.

After both passes, any tag_ref whose previously-resolved tag has since
been deleted is re-marked drifted. Drift is reported in the collect log.
"""
from __future__ import annotations

import logging
import re
from pathlib import Path
from typing import Optional

from core.db import upsert_tag, upsert_tag_ref, resolve_tag, open_db
from core.regions import extract_regions

LOG = logging.getLogger("collect.tags")

_SOURCE_EXTS = {
    ".py", ".scd", ".sc", ".js", ".ts", ".cpp", ".cc", ".cxx",
    ".c", ".h", ".hpp", ".rs", ".go", ".rb", ".lua", ".sql",
    ".sh", ".bash", ".zsh", ".hs", ".ml", ".jl", ".r", ".java",
    ".cs", ".yaml", ".yml", ".toml",
}

_SKIP_DIRS = {
    ".git", "__pycache__", "node_modules", ".venv", "venv",
    "build", "dist", ".mypy_cache", ".ruff_cache", "target",
    ".tox", ".eggs", ".direnv",
}

# Matches {@region: name}, {@region: repo/name}, {@region: file#name}
_DIRECTIVE_RE = re.compile(
    r"\{@region\s*:\s*([^}]+)\}",
    re.IGNORECASE,
)


def _skip(path: Path, repo_root: Path) -> bool:
    try:
        rel = path.relative_to(repo_root)
    except ValueError:
        return False
    return any(part in _SKIP_DIRS for part in rel.parts)


def collect_tags(
    *,
    db_path: Path,
    watches: list[dict],
    prose_roots: list[Path],
    default_repo_id: Optional[str] = None,
) -> dict:
    """
    Main entry point.

    watches       — list of {id, path, visibility} dicts
    prose_roots   — directories to scan for .md files
    default_repo_id — fallback repo for bare name resolution
    """
    con = open_db(db_path)

    total_tags    = 0
    total_files   = 0
    total_refs    = 0
    total_drifted = 0

    # ── Pass 1: source files → tags ────────────────────────────────
    for watch in watches:
        repo_id   = watch["id"]
        repo_root = Path(watch["path"])

        if not repo_root.exists():
            LOG.warning("collect.tags: watch path not found: %s", repo_root)
            continue

        repo_tags  = 0
        repo_files = 0

        for src in repo_root.rglob("*"):
            if not src.is_file():
                continue
            if _skip(src, repo_root):
                continue
            if src.suffix.lower() not in _SOURCE_EXTS:
                continue

            try:
                regions = extract_regions(src)
            except Exception as e:
                LOG.debug("collect.tags: failed to parse %s: %s", src, e)
                continue

            if not regions:
                continue

            rel = src.relative_to(repo_root).as_posix()
            repo_files += 1

            for name, region in regions.items():
                upsert_tag(
                    con,
                    repo_id=repo_id,
                    name=name,
                    source_file=rel,
                    language=region.language,
                    sha=region.sha,
                    prose=region.prose or None,
                )
                repo_tags += 1

        total_tags  += repo_tags
        total_files += repo_files
        LOG.info(
            "collect.tags: %s → %d tags from %d source files",
            repo_id, repo_tags, repo_files,
        )

    # ── Pass 2: prose files → tag_refs ─────────────────────────────
    seen: set[str] = set()

    for prose_root in prose_roots:
        if not prose_root.exists():
            continue

        for md_file in prose_root.rglob("*.md"):
            if not md_file.is_file():
                continue
            key = str(md_file.resolve())
            if key in seen:
                continue
            seen.add(key)

            # Skip files inside _SKIP_DIRS
            skip = False
            for part in md_file.parts:
                if part in _SKIP_DIRS:
                    skip = True
                    break
            if skip:
                continue

            try:
                text = md_file.read_text(encoding="utf-8", errors="replace")
            except Exception:
                continue

            for raw_arg in _DIRECTIVE_RE.findall(text):
                raw_arg = raw_arg.strip()

                # file#name form — no DB resolution needed, not a tag ref
                if "#" in raw_arg:
                    continue

                name = raw_arg
                tag = resolve_tag(
                    con, name,
                    prose_file=md_file,
                    default_repo_id=default_repo_id,
                )

                if tag is not None:
                    upsert_tag_ref(
                        con,
                        prose_file=str(md_file),
                        name=name,
                        tag_id=tag["id"],
                        repo_id=tag["repo_id"],
                        drifted=False,
                    )
                    total_refs += 1
                else:
                    # Check ambiguity vs missing
                    candidates = con.execute(
                        "SELECT id FROM tags WHERE name = ?", (name,)
                    ).fetchall()
                    if candidates:
                        LOG.warning(
                            "collect.tags: ambiguous {@region: %s} in %s — matches: %s",
                            name, md_file.name, [r["id"] for r in candidates],
                        )
                    else:
                        LOG.warning(
                            "collect.tags: drifted {@region: %s} in %s",
                            name, md_file.name,
                        )
                        upsert_tag_ref(
                            con,
                            prose_file=str(md_file),
                            name=name,
                            tag_id=None,
                            repo_id=None,
                            drifted=True,
                        )
                        total_drifted += 1

    # ── Re-check existing refs for drift ───────────────────────────
    con.execute(
        """
        UPDATE tag_refs
        SET drifted = 1, tag_id = NULL, repo_id = NULL
        WHERE tag_id IS NOT NULL
          AND tag_id NOT IN (SELECT id FROM tags)
        """
    )

    con.commit()
    con.close()

    return {
        "tags":    total_tags,
        "files":   total_files,
        "refs":    total_refs,
        "drifted": total_drifted,
    }
