"""Copy pre-rename project state without replacing or rewriting existing data."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import tempfile


LEGACY_FILES = {
    ".obstral.md": ".spiral-coder.md",
    ".obstralignore": ".spiral-coderignore",
    ".obstral_history": ".spiral_coder_history",
    ".tmp/obstral_session.json": ".tmp/spiral_coder_session.json",
    ".tmp/obstral_trace.jsonl": ".tmp/spiral_coder_trace.jsonl",
    ".tmp/obstral_final.json": ".tmp/spiral_coder_final.json",
    ".tmp/obstral_graph.json": ".tmp/spiral_coder_graph.json",
}


def _contains_symlink(path: Path, root: Path) -> bool:
    return any(part.is_symlink() for part in (path, *path.parents) if part != root and root in part.parents)


def migrate(root: Path, *, apply: bool = False) -> list[tuple[str, str, str]]:
    """Return (status, source, destination); conflicts and symlinks are preserved."""
    root = root.resolve(strict=True)
    if not root.is_dir():
        raise ValueError("root must be a directory")
    pairs = [(root / old, root / new) for old, new in LEGACY_FILES.items()]
    legacy_dir = root / ".obstral"
    if legacy_dir.is_symlink():
        pairs.append((legacy_dir, root / ".spiral-coder"))
    elif legacy_dir.is_dir():
        pairs.extend(
            (p, root / ".spiral-coder" / p.relative_to(legacy_dir))
            for p in sorted(legacy_dir.rglob("*"))
            if p.is_file() or p.is_symlink()
        )

    results = []
    for source, destination in pairs:
        if not source.exists() and not source.is_symlink():
            continue
        if _contains_symlink(source, root) or _contains_symlink(destination, root):
            status = "skip-symlink"
        elif destination.exists():
            status = "keep-existing"
        elif not source.is_file():
            status = "skip-nonfile"
        elif any(p.exists() and not p.is_dir() for p in destination.parents if p != root):
            status = "keep-existing-parent"
        elif apply:
            destination.parent.mkdir(parents=True, exist_ok=True)
            # Publish a complete copy atomically and without replacing any destination.
            # A failed copy must not leave a partial file that the next run will skip.
            temporary = None
            try:
                with source.open("rb") as src, tempfile.NamedTemporaryFile(
                    dir=destination.parent, prefix=".spiral-coder-migrate-", delete=False
                ) as dst:
                    temporary = Path(dst.name)
                    shutil.copyfileobj(src, dst)
                    dst.flush()
                    os.fsync(dst.fileno())
                shutil.copystat(source, temporary)
                os.link(temporary, destination)
            except FileExistsError:
                status = "keep-existing"
            else:
                status = "copied"
            finally:
                if temporary is not None:
                    temporary.unlink(missing_ok=True)
        else:
            status = "would-copy"
        results.append((status, str(source.relative_to(root)), str(destination.relative_to(root))))
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd(), help="Project directory to migrate")
    parser.add_argument("--apply", action="store_true", help="Copy files; default is a dry run")
    args = parser.parse_args()
    try:
        results = migrate(args.root, apply=args.apply)
    except (OSError, ValueError) as exc:
        parser.exit(1, f"migration failed: {exc}\n")
    for status, source, destination in results:
        print(f"{status}: {source} -> {destination}")
    if not results:
        print("No legacy state found.")
    elif not args.apply:
        print("Dry run. Use --apply to copy; existing destinations are never replaced.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
