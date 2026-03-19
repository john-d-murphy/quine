"""
core/regions.py — Extract named regions from source files.

A region is a block in a source file delimited by:

    // @@region my_name          (JS, C++, SC, Java)
    # @@region my_name           (Python, Ruby, shell)
    -- @@region my_name          (SQL, Lua, Haskell)
    ; @@region my_name           (Lisp, Clojure)

    // @@endregion               (any of the above comment styles)
    # @@endregion
    etc.

The region contains the raw lines between the markers — code and
comments alike. Leading comment lines before the first non-comment,
non-blank line in the region are extracted as "prose": they become
the narrative text in the rendered document. The remaining lines are
the "code" shown in a syntax-highlighted block.

The source file is never modified.
"""
from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

# Comment prefixes recognised as prose markers
_COMMENT_PREFIXES = ("//", "#", "--", ";", "/*", "*")

# Marker patterns
_REGION_RE    = re.compile(r"^\s*(?://|#|--|;|\*)\s*@region\s+(\S+)")
_ENDREGION_RE = re.compile(r"^\s*(?://|#|--|;|\*)\s*@endregion")

# Language detection by extension
_EXT_TO_LANG: dict[str, str] = {
    ".py":   "python",
    ".scd":  "supercollider",
    ".sc":   "supercollider",
    ".js":   "javascript",
    ".ts":   "typescript",
    ".cpp":  "cpp",
    ".cc":   "cpp",
    ".cxx":  "cpp",
    ".c":    "c",
    ".h":    "cpp",
    ".hpp":  "cpp",
    ".rs":   "rust",
    ".go":   "go",
    ".rb":   "ruby",
    ".lua":  "lua",
    ".sql":  "sql",
    ".sh":   "bash",
    ".bash": "bash",
    ".zsh":  "bash",
    ".hs":   "haskell",
    ".ml":   "ocaml",
    ".jl":   "julia",
    ".r":    "r",
    ".java": "java",
    ".cs":   "csharp",
    ".md":   "markdown",
    ".yaml": "yaml",
    ".yml":  "yaml",
    ".toml": "toml",
}


@dataclass
class Region:
    name:     str
    file:     Path          # absolute path to source file
    language: str           # syntax highlighting hint
    sha:      str           # sha1 of file contents at extraction time

    # Lines as they appear in the source (including comment markers)
    raw_lines: list[str] = field(default_factory=list)

    # Split view produced by split()
    prose_lines: list[str] = field(default_factory=list)  # stripped of comment prefix
    code_lines:  list[str] = field(default_factory=list)  # as-is from source

    @property
    def prose(self) -> str:
        """Leading comment block, stripped of comment markers."""
        return "\n".join(self.prose_lines).strip()

    @property
    def code(self) -> str:
        """Code body (may still contain inline comments)."""
        return "\n".join(self.code_lines).rstrip()

    @property
    def has_prose(self) -> bool:
        return bool(self.prose_lines)

    @property
    def has_code(self) -> bool:
        return bool(self.code_lines)


def _detect_language(path: Path) -> str:
    return _EXT_TO_LANG.get(path.suffix.lower(), "text")


def _strip_comment(line: str) -> Optional[str]:
    """
    If *line* is a pure comment (possibly indented), return the text
    after the comment marker. Otherwise return None.
    """
    stripped = line.lstrip()
    for prefix in _COMMENT_PREFIXES:
        if stripped.startswith(prefix):
            after = stripped[len(prefix):]
            # Normalise one leading space
            if after.startswith(" "):
                after = after[1:]
            return after
    return None


# @region _split_region
# Each region is split into two parts: a prose block and a code block.
# Prose is the leading run of comment lines, stripped of their markers.
# Code is everything after the first non-comment, non-blank line.
# A region can be all prose, all code, or both — the renderer handles each case.
def _split_region(raw_lines: list[str]) -> tuple[list[str], list[str]]:
    """
    Split region lines into (prose_lines, code_lines).

    Leading contiguous comment lines → prose (stripped of markers).
    Everything after the first non-comment, non-blank line → code.
    Blank lines within the leading comment block are included in prose.
    """
    prose: list[str] = []
    code:  list[str] = []
    in_prose = True

    for line in raw_lines:
        stripped = line.strip()
        if not stripped:
            # blank line: stays in prose while in_prose, else in code
            if in_prose:
                prose.append("")
            else:
                code.append(line.rstrip())
            continue

        if in_prose:
            comment_text = _strip_comment(line)
            if comment_text is not None:
                prose.append(comment_text)
            else:
                in_prose = False
                code.append(line.rstrip())
        else:
            code.append(line.rstrip())

    # Trim trailing blanks from prose
    while prose and not prose[-1]:
        prose.pop()

    return prose, code
# @endregion


# @region extract_regions
# Scan a source file for @region/@endregion markers and return all
# named regions it contains. Regions can be nested — inner regions
# are captured independently. The file is read once and never modified.
def extract_regions(path: Path) -> dict[str, Region]:
    """
    Parse *path* for @region/@endregion markers.
    Returns {region_name: Region}.
    """
    path = path.resolve()
    if not path.exists():
        return {}

    raw = path.read_bytes()
    sha = hashlib.sha1(raw).hexdigest()
    text = raw.decode("utf-8", errors="replace")
    lang = _detect_language(path)

    regions: dict[str, Region] = {}
    stack: list[tuple[str, list[str]]] = []  # [(name, lines), ...]

    for line in text.splitlines():
        m_start = _REGION_RE.match(line)
        m_end   = _ENDREGION_RE.match(line)

        if m_start:
            name = m_start.group(1).strip()
            stack.append((name, []))
        elif m_end and stack:
            name, raw_lines = stack.pop()
            prose, code = _split_region(raw_lines)
            regions[name] = Region(
                name=name,
                file=path,
                language=lang,
                sha=sha,
                raw_lines=raw_lines,
                prose_lines=prose,
                code_lines=code,
            )
        elif stack:
            # Inside one or more regions — append to innermost
            stack[-1][1].append(line)

    return regions


# @endregion

def extract_region(path: Path, name: str) -> Optional[Region]:
    """Convenience: extract a single named region."""
    return extract_regions(path).get(name)
