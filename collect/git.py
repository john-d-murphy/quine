"""
collect/git.py — Ingest git commits from watched repos into items.

Each commit becomes one item:
  source      = 'git'
  watch_id    = repo id
  ref         = full sha
  title       = first line of commit message
  occurred_at = author date (ISO)
  meta        = {sha, sha_short, author, email, body}
"""

from __future__ import annotations

import subprocess
import logging
from pathlib import Path

from core.db import upsert_item

LOG = logging.getLogger("collect.git")

_GIT_LOG_FMT = "%H\x1f%h\x1f%an\x1f%ae\x1f%aI\x1f%s\x1f%b\x1e"


def ingest_repo(
    con, *, watch_id: str, repo_path: Path, visibility: str = "private"
) -> int:
    """Walk git log for repo_path, upsert all commits. Returns count ingested."""
    if not (repo_path / ".git").exists():
        LOG.warning("collect.git: not a git repo: %s", repo_path)
        return 0

    try:
        result = subprocess.run(
            ["git", "log", f"--format={_GIT_LOG_FMT}", "--all"],
            cwd=repo_path,
            capture_output=True,
            text=True,
            timeout=30,
        )
    except Exception as e:
        LOG.warning("collect.git: git log failed for %s: %s", repo_path, e)
        return 0

    count = 0
    for record in result.stdout.split("\x1e"):
        record = record.strip()
        if not record:
            continue
        parts = record.split("\x1f", 6)
        if len(parts) < 6:
            continue

        sha, sha_short, author, email, date, subject = parts[:6]
        body = parts[6].strip() if len(parts) > 6 else ""

        upsert_item(
            con,
            id=f"git:{watch_id}:{sha}",
            source="git",
            watch_id=watch_id,
            ref=sha,
            title=subject.strip(),
            occurred_at=date.strip(),
            visibility=visibility,
            meta={
                "sha": sha,
                "sha_short": sha_short,
                "author": author,
                "email": email,
                "body": body,
            },
        )
        count += 1

    LOG.info("collect.git: %s → %d commits", watch_id, count)
    return count
