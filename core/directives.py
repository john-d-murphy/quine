"""
core/directives.py — Expand {@directive} markers in markdown text.

Directives are expanded before markdown rendering. They pull named regions
from source files into prose documents without modifying either side.

Supported directives
--------------------
{@region: name}
    Bare name. Resolved by proximity → default repo → singleton.

{@region: repo_id/name}
    Qualified name. Always unambiguous.

{@region: path/to/file.py#name}
    Explicit file + name. Bypasses the tag registry entirely.

Resolution order for bare names
--------------------------------
1. Proximity  — tag in same repo as the prose file (path prefix match)
2. Default    — watch with default_repo=1 in config
3. Singleton  — only one tag with that name across all repos
4. Ambiguous  — rendered as warning block

Rendered output per directive
------------------------------
1. Prose paragraph (if region has leading comment block)
2. Attribution line: filename · region_name  
3. Fenced code block with language hint
4. Download link: /source/{repo_id}/{relpath}
"""
from __future__ import annotations

import re
import sqlite3
from pathlib import Path
from typing import Optional

from core.regions import Region, extract_region, _detect_language

_DIRECTIVE_RE = re.compile(
    r"\{@(?P<verb>region)\s*:\s*(?P<arg>[^}]+)\}",
    re.IGNORECASE,
)


def _render_region(
    region: Region,
    *,
    source_file: Path,
    repo_id: str,
    rel_path: str,
) -> str:
    parts: list[str] = []

    if region.has_prose:
        paras: list[list[str]] = []
        current: list[str] = []
        for line in region.prose_lines:
            if not line.strip():
                if current:
                    paras.append(current)
                    current = []
            else:
                current.append(line)
        if current:
            paras.append(current)
        for para in paras:
            parts.append(" ".join(para))
            parts.append("")

    if region.has_code:
        lang = region.language or ""
        parts.append(f"<div class='region-attr'>{source_file.name} · {region.name}</div>")
        parts.append(f"```{lang}")
        parts.append(region.code)
        parts.append("```")
        parts.append("")

    download_url = f"/source/{repo_id}/{rel_path}" if repo_id else f"/source/{rel_path}"
    parts.append(
        f"<div class='region-download'>"
        f"<a href='{download_url}' download>⬇ {source_file.name}</a>"
        f"</div>"
    )
    parts.append("")
    return "\n".join(parts)


def _render_missing(arg: str, reason: str) -> str:
    return (
        f"<div class='region-missing'>"
        f"<code>{{@region: {arg}}}</code> — {reason}"
        f"</div>\n"
    )


def _render_ambiguous(name: str, candidates: list[str]) -> str:
    opts = ", ".join(f"<code>{c}</code>" for c in candidates)
    return (
        f"<div class='region-ambiguous'>"
        f"<code>{{@region: {name}}}</code> — ambiguous ({opts}). "
        f"Use a qualified name."
        f"</div>\n"
    )


def _find_file(file_str: str, *, base: Optional[Path]) -> Optional[Path]:
    p = Path(file_str)
    if p.is_absolute() and p.exists():
        return p
    if base:
        candidate = (base.parent / p).resolve()
        if candidate.exists():
            return candidate
    if p.exists():
        return p.resolve()
    return None


def process_directives(
    md_text: str,
    *,
    md_path: Optional[Path] = None,
    con: Optional[sqlite3.Connection] = None,
    annotates: Optional[Path] = None,
    default_repo_id: Optional[str] = None,
) -> str:
    """
    Expand all {@region:} directives in md_text.

    con             — open DB connection for tag resolution
    md_path         — absolute path to the prose file (proximity resolution)
    annotates       — explicit source file from front matter (fallback)
    default_repo_id — fallback repo for bare names
    """
    def replace(m: re.Match) -> str:
        verb = m.group("verb").lower()
        arg  = m.group("arg").strip()

        if verb != "region":
            return m.group(0)

        # Case 1: explicit file#name — bypasses registry
        if "#" in arg:
            file_part, region_name = arg.rsplit("#", 1)
            file_path = _find_file(file_part.strip(), base=md_path)
            if file_path is None:
                return _render_missing(arg, f"file not found: {file_part}")
            region = extract_region(file_path, region_name.strip())
            if region is None:
                return _render_missing(arg, f"region '{region_name}' not found in {file_path.name}")
            return _render_region(region, source_file=file_path, repo_id="", rel_path=file_path.name)

        # Case 2: DB-backed resolution
        if con is not None:
            from core.db import resolve_tag
            tag = resolve_tag(con, arg, prose_file=md_path, default_repo_id=default_repo_id)

            if tag is None and "/" not in arg:
                candidates = con.execute(
                    "SELECT id FROM tags WHERE name = ?", (arg,)
                ).fetchall()
                if candidates:
                    return _render_ambiguous(arg, [r["id"] for r in candidates])
                return _render_missing(arg, f"tag '{arg}' not found in any watched repo")

            if tag is None:
                return _render_missing(arg, f"tag '{arg}' not found")

            watch = con.execute(
                "SELECT path FROM watches WHERE id = ?", (tag["repo_id"],)
            ).fetchone()
            if not watch:
                return _render_missing(arg, f"watch '{tag['repo_id']}' not found")

            file_path = Path(watch["path"]) / tag["source_file"]
            if not file_path.exists():
                return _render_missing(arg, f"source file missing: {tag['source_file']}")

            region = extract_region(file_path, tag["name"])
            if region is None:
                return _render_missing(arg, f"region '{tag['name']}' vanished from source")

            return _render_region(
                region,
                source_file=file_path,
                repo_id=tag["repo_id"],
                rel_path=tag["source_file"],
            )

        # Case 3: no DB, use annotates file directly
        if annotates is not None:
            region = extract_region(annotates, arg)
            if region is None:
                return _render_missing(arg, f"region '{arg}' not found in {annotates.name}")
            return _render_region(region, source_file=annotates, repo_id="", rel_path=annotates.name)

        return _render_missing(arg, "no DB connection and no annotates file")

    return _DIRECTIVE_RE.sub(replace, md_text)


def parse_front_matter_annotates(
    fm: dict, *, md_path: Optional[Path] = None
) -> Optional[Path]:
    raw = fm.get("annotates") or fm.get("annotates_file")
    if not raw:
        return None
    return _find_file(str(raw), base=md_path)
