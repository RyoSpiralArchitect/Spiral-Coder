#!/usr/bin/env python3
"""Repository map v1 with symbol indexing and partial file loading.

This tool is intentionally lightweight:
  - `build`: scan the repo and write a compact symbol/index cache
  - `query`: rank likely target files/symbols for a free-text query
  - `show`: load only a symbol span or an explicit line range

v1 favors cheap, deterministic parsing over perfect semantic understanding.
"""

from __future__ import annotations

import argparse
import ast
import difflib
import fnmatch
import hashlib
import json
import os
import re
import sys
import time
from pathlib import Path, PurePosixPath
from typing import Any, Dict, Iterable, List, Optional, Sequence, Tuple


VERSION = 1
DEFAULT_INDEX = ".spiral/repo_map.json"
DEFAULT_CONFIG = ".spiral-coder/repo_map.config.json"
DEFAULT_EVAL = ".spiral-coder/repo_map.eval.json"
DEFAULT_IGNORE = ".spiral-coderignore"
DEFAULT_MAX_BYTES = 4_000_000

DEFAULT_SKIP_DIRS = {
    ".git",
    ".hg",
    ".svn",
    ".buildenv",
    ".venv",
    ".cargo",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".idea",
    ".next",
    ".cache",
    "node_modules",
    "dist",
    "build",
    "vendor",
    "site-packages",
    "target",
    "registry",
    "cargo-home",
    "runs",
    "data",
    "val",
    "_out",
    "out",
}

LANG_BY_EXT = {
    ".py": "python",
    ".pyi": "python",
    ".ts": "typescript",
    ".tsx": "typescript",
    ".js": "javascript",
    ".jsx": "javascript",
    ".mjs": "javascript",
    ".cjs": "javascript",
    ".rs": "rust",
    ".go": "go",
    ".java": "java",
    ".c": "c",
    ".cc": "cpp",
    ".cpp": "cpp",
    ".cxx": "cpp",
    ".h": "cpp",
    ".hh": "cpp",
    ".hpp": "cpp",
}

LEXEME_STOPWORDS = {
    "and", "any", "arg", "args", "array", "async", "await", "bool", "break", "case",
    "catch", "char", "class", "const", "continue", "crate", "default", "def", "dict",
    "elif", "else", "enum", "error", "export", "false", "final", "finally", "float",
    "fn", "for", "from", "func", "function", "if", "impl", "import", "in", "int",
    "interface", "let", "list", "loop", "match", "mod", "module", "mut", "new", "nil",
    "none", "not", "null", "object", "or", "package", "pass", "pub", "raise", "return",
    "self", "static", "string", "struct", "super", "switch", "this", "throw", "trait",
    "true", "try", "type", "use", "var", "vec", "void", "while", "with", "yield",
}

PATHISH_HINT_TERMS = {
    "c", "cc", "cpp", "cxx", "go", "h", "hh", "hpp", "java", "js", "jsx",
    "mjs", "py", "pyi", "rs", "ts", "tsx",
}

IMPORT_RE = re.compile(
    r"""(?mx)
    ^
    \s*
    (?:
        import\s+.*?\s+from\s+["']([^"']+)["']
      |
        export\s+.*?\s+from\s+["']([^"']+)["']
      |
        import\s+["']([^"']+)["']
      |
        (?:const|let|var)\s+\w+\s*=\s*require\(\s*["']([^"']+)["']\s*\)
      |
        use\s+([A-Za-z0-9_:]+)\s*;
      |
        #include\s*[<"]([^>"]+)[>"]
    )
    """
)

TOP_LEVEL_DECL_RE = re.compile(
    r"""(?mx)
    ^
    (?:
        (?P<ts_func>export\s+(?:default\s+)?(?:async\s+)?function\s+(?P<ts_func_name>[A-Za-z_][$\w]*)\s*\()
      |
        (?P<ts_class>export\s+(?:default\s+)?class\s+(?P<ts_class_name>[A-Za-z_][$\w]*))
      |
        (?P<ts_arrow>export\s+(?:const|let|var)\s+(?P<ts_arrow_name>[A-Za-z_][$\w]*)\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_][$\w]*)\s*=>)
      |
        (?P<ts_plain_func>(?:async\s+)?function\s+(?P<ts_plain_func_name>[A-Za-z_][$\w]*)\s*\()
      |
        (?P<ts_plain_class>class\s+(?P<ts_plain_class_name>[A-Za-z_][$\w]*))
      |
        (?P<rust_fn>(?:pub\s+)?fn\s+(?P<rust_fn_name>[A-Za-z_][\w]*)\s*\()
      |
        (?P<rust_struct>(?:pub\s+)?struct\s+(?P<rust_struct_name>[A-Za-z_][\w]*))
      |
        (?P<rust_enum>(?:pub\s+)?enum\s+(?P<rust_enum_name>[A-Za-z_][\w]*))
      |
        (?P<rust_trait>(?:pub\s+)?trait\s+(?P<rust_trait_name>[A-Za-z_][\w]*))
      |
        (?P<rust_impl>impl(?:<[^>]+>)?\s+(?P<rust_impl_name>[A-Za-z_][\w:<>]*)\s*)
    )
    """
)


def normalize_terms(text: str) -> List[str]:
    seen: set[str] = set()
    out: List[str] = []
    raw_tokens = re.findall(r"[A-Za-z0-9_]+", text)
    for raw in raw_tokens:
        candidates = [raw.lower()]
        for piece in re.split(r"_+", raw):
            if not piece:
                continue
            parts = re.findall(r"[A-Z]+(?=[A-Z][a-z]|$)|[A-Z]?[a-z]+|[0-9]+", piece)
            candidates.extend(p.lower() for p in parts if p)
        for token in candidates:
            if not token:
                continue
            if len(token) < 2 and not token.isdigit():
                continue
            if token in LEXEME_STOPWORDS:
                continue
            if token not in seen:
                seen.add(token)
                out.append(token)
    return out


def clamp01(value: float) -> float:
    return max(0.0, min(1.0, value))


def confidence_label(value: float) -> str:
    if value >= 0.8:
        return "high"
    if value >= 0.55:
        return "medium"
    return "low"


def default_settings() -> Dict[str, Any]:
    return {
        "index_path": DEFAULT_INDEX,
        "ignore_file": DEFAULT_IGNORE,
        "eval_file": DEFAULT_EVAL,
        "scan": {
            "max_bytes": DEFAULT_MAX_BYTES,
            "skip_dirs": [],
            "ignore_globs": [],
        },
        "query": {
            "top_k": 10,
        },
    }


def deep_merge(base: Dict[str, Any], override: Dict[str, Any]) -> Dict[str, Any]:
    merged = dict(base)
    for key, value in override.items():
        if isinstance(value, dict) and isinstance(merged.get(key), dict):
            merged[key] = deep_merge(merged[key], value)
        else:
            merged[key] = value
    return merged


def load_json(path: Path) -> Dict[str, Any]:
    if not path.exists():
        return {}
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"failed to parse json {path}: {exc}") from exc
    if not isinstance(payload, dict):
        raise SystemExit(f"expected JSON object in {path}")
    return payload


def resolve_repo_settings(root: Path, config_path_arg: Optional[str]) -> Tuple[Dict[str, Any], Path]:
    config_path = Path(config_path_arg) if config_path_arg else root / DEFAULT_CONFIG
    if not config_path.is_absolute():
        config_path = root / config_path
    settings = default_settings()
    if config_path.exists():
        settings = deep_merge(settings, load_json(config_path))
    return settings, config_path


def resolve_path(root: Path, value: str) -> Path:
    path = Path(value)
    if not path.is_absolute():
        path = root / path
    return path


def resolve_index_path(root: Path, settings: Dict[str, Any], index_arg: Optional[str]) -> Path:
    index_value = index_arg or settings.get("index_path") or DEFAULT_INDEX
    return resolve_path(root, index_value)


def load_ignore_patterns(root: Path, settings: Dict[str, Any], ignore_file_arg: Optional[str]) -> List[str]:
    ignore_patterns = [str(x).replace("\\", "/") for x in settings.get("scan", {}).get("ignore_globs", [])]
    ignore_file_value = ignore_file_arg or settings.get("ignore_file") or DEFAULT_IGNORE
    ignore_path = resolve_path(root, ignore_file_value)
    if ignore_path.exists():
        for raw in ignore_path.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            ignore_patterns.append(line.replace("\\", "/"))
    return ignore_patterns


def path_matches_pattern(rel_path: str, pattern: str) -> bool:
    rel = rel_path.strip("/")
    pat = pattern.strip()
    if not rel or not pat:
        return False
    if pat.endswith("/"):
        pat = pat.rstrip("/") + "/**"
    if fnmatch.fnmatch(rel, pat):
        return True
    if "/" not in pat and fnmatch.fnmatch(Path(rel).name, pat):
        return True
    try:
        if PurePosixPath(rel).match(pat):
            return True
    except Exception:
        pass
    return False


def is_ignored(rel_path: str, ignore_patterns: Sequence[str]) -> bool:
    return any(path_matches_pattern(rel_path, pattern) for pattern in ignore_patterns)


def sha1_text(text: str) -> str:
    return hashlib.sha1(text.encode("utf-8", errors="ignore")).hexdigest()


def detect_language(path: Path) -> Optional[str]:
    return LANG_BY_EXT.get(path.suffix.lower())


def should_skip_dir(name: str, extra_skip_dirs: Sequence[str]) -> bool:
    return name in DEFAULT_SKIP_DIRS or name in set(extra_skip_dirs)


def iter_code_files(
    root: Path,
    max_bytes: int,
    extra_skip_dirs: Sequence[str],
    ignore_patterns: Sequence[str],
) -> Iterable[Path]:
    for dirpath, dirnames, filenames in os.walk(root):
        dir_rel = Path(dirpath).resolve().relative_to(root).as_posix() if Path(dirpath).resolve() != root else ""
        if dir_rel and is_ignored(dir_rel, ignore_patterns):
            dirnames[:] = []
            continue
        dirnames[:] = [
            d for d in dirnames
            if not should_skip_dir(d, extra_skip_dirs)
            and not d.startswith(".git")
            and not is_ignored((f"{dir_rel}/{d}" if dir_rel else d), ignore_patterns)
        ]
        base = Path(dirpath)
        for filename in filenames:
            path = base / filename
            rel = path.resolve().relative_to(root).as_posix()
            if is_ignored(rel, ignore_patterns):
                continue
            lang = detect_language(path)
            if lang is None:
                continue
            try:
                size = path.stat().st_size
            except OSError:
                continue
            if size > max_bytes:
                continue
            yield path


def offset_to_line(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def line_start_offsets(text: str) -> List[int]:
    starts = [0]
    for i, ch in enumerate(text):
        if ch == "\n":
            starts.append(i + 1)
    return starts


def find_matching_brace(text: str, open_pos: int) -> Optional[int]:
    depth = 0
    i = open_pos
    in_single = False
    in_double = False
    in_backtick = False
    in_line_comment = False
    in_block_comment = False
    while i < len(text):
        ch = text[i]
        nxt = text[i + 1] if i + 1 < len(text) else ""

        if in_line_comment:
            if ch == "\n":
                in_line_comment = False
            i += 1
            continue
        if in_block_comment:
            if ch == "*" and nxt == "/":
                in_block_comment = False
                i += 2
            else:
                i += 1
            continue
        if in_single:
            if ch == "\\":
                i += 2
                continue
            if ch == "'":
                in_single = False
            i += 1
            continue
        if in_double:
            if ch == "\\":
                i += 2
                continue
            if ch == '"':
                in_double = False
            i += 1
            continue
        if in_backtick:
            if ch == "\\":
                i += 2
                continue
            if ch == "`":
                in_backtick = False
            i += 1
            continue

        if ch == "/" and nxt == "/":
            in_line_comment = True
            i += 2
            continue
        if ch == "/" and nxt == "*":
            in_block_comment = True
            i += 2
            continue
        if ch == "'":
            in_single = True
            i += 1
            continue
        if ch == '"':
            in_double = True
            i += 1
            continue
        if ch == "`":
            in_backtick = True
            i += 1
            continue

        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return None


def symbol_end_line_from_braces(text: str, match_end: int, fallback_start: int) -> int:
    brace_pos = text.find("{", match_end)
    if brace_pos == -1:
        return fallback_start
    close_pos = find_matching_brace(text, brace_pos)
    if close_pos is None:
        return fallback_start
    return offset_to_line(text, close_pos)


def parse_python(text: str) -> Tuple[List[Dict[str, Any]], List[str], Optional[str]]:
    symbols: List[Dict[str, Any]] = []
    imports: set[str] = set()
    try:
        tree = ast.parse(text)
    except SyntaxError as exc:
        return symbols, [], f"syntax-error:{exc.lineno}:{exc.offset}"

    class Visitor(ast.NodeVisitor):
        def __init__(self) -> None:
            self.stack: List[str] = []

        def _add_symbol(self, node: ast.AST, name: str, kind: str) -> None:
            qualname = ".".join(self.stack + [name]) if self.stack else name
            symbols.append(
                {
                    "name": name,
                    "qualname": qualname,
                    "kind": kind,
                    "lineno": int(getattr(node, "lineno", 1)),
                    "end_lineno": int(getattr(node, "end_lineno", getattr(node, "lineno", 1))),
                }
            )

        def visit_Import(self, node: ast.Import) -> None:
            for alias in node.names:
                imports.add(alias.name.split(".")[0])

        def visit_ImportFrom(self, node: ast.ImportFrom) -> None:
            if node.module:
                imports.add(node.module.split(".")[0])

        def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
            self._add_symbol(node, node.name, "function")
            self.stack.append(node.name)
            self.generic_visit(node)
            self.stack.pop()

        def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
            self._add_symbol(node, node.name, "async_function")
            self.stack.append(node.name)
            self.generic_visit(node)
            self.stack.pop()

        def visit_ClassDef(self, node: ast.ClassDef) -> None:
            self._add_symbol(node, node.name, "class")
            self.stack.append(node.name)
            self.generic_visit(node)
            self.stack.pop()

    Visitor().visit(tree)
    return symbols, sorted(imports), None


def parse_brace_lang(text: str) -> Tuple[List[Dict[str, Any]], List[str], Optional[str]]:
    symbols: List[Dict[str, Any]] = []
    imports: set[str] = set()

    for match in IMPORT_RE.finditer(text):
        for grp in match.groups():
            if grp:
                imports.add(grp.split("/")[0].split("::")[0])
                break

    for match in TOP_LEVEL_DECL_RE.finditer(text):
        line = offset_to_line(text, match.start())
        kind = ""
        name = ""
        if match.group("ts_func_name"):
            kind = "function"
            name = match.group("ts_func_name")
        elif match.group("ts_class_name"):
            kind = "class"
            name = match.group("ts_class_name")
        elif match.group("ts_arrow_name"):
            kind = "const"
            name = match.group("ts_arrow_name")
        elif match.group("ts_plain_func_name"):
            kind = "function"
            name = match.group("ts_plain_func_name")
        elif match.group("ts_plain_class_name"):
            kind = "class"
            name = match.group("ts_plain_class_name")
        elif match.group("rust_fn_name"):
            kind = "function"
            name = match.group("rust_fn_name")
        elif match.group("rust_struct_name"):
            kind = "struct"
            name = match.group("rust_struct_name")
        elif match.group("rust_enum_name"):
            kind = "enum"
            name = match.group("rust_enum_name")
        elif match.group("rust_trait_name"):
            kind = "trait"
            name = match.group("rust_trait_name")
        elif match.group("rust_impl_name"):
            kind = "impl"
            name = match.group("rust_impl_name")
        if not name:
            continue
        end_line = symbol_end_line_from_braces(text, match.end(), line)
        symbols.append(
            {
                "name": name,
                "qualname": name,
                "kind": kind,
                "lineno": line,
                "end_lineno": max(line, end_line),
            }
        )
    return symbols, sorted(imports), None


def parse_file(path: Path, lang: str) -> Dict[str, Any]:
    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except OSError as exc:
        return {"parse_error": f"io-error:{exc}"}

    if lang == "python":
        symbols, imports, parse_error = parse_python(text)
    else:
        symbols, imports, parse_error = parse_brace_lang(text)

    return {
        "size": len(text.encode("utf-8", errors="ignore")),
        "line_count": text.count("\n") + 1,
        "sha1": sha1_text(text),
        "symbols": symbols,
        "imports": imports,
        "lexemes": extract_lexemes(text),
        "parse_error": parse_error,
    }


def extract_lexemes(text: str, limit: int = 64) -> List[str]:
    counts: Dict[str, int] = {}
    for token in re.findall(r"[A-Za-z_][A-Za-z0-9_]{2,}", text):
        low = token.lower()
        if low in LEXEME_STOPWORDS:
            continue
        counts[low] = counts.get(low, 0) + 1
    ranked = sorted(counts.items(), key=lambda item: (-item[1], item[0]))
    return [token for token, _count in ranked[:limit]]


def build_index(
    root: Path,
    out_path: Path,
    max_bytes: int,
    extra_skip_dirs: Sequence[str],
    ignore_patterns: Sequence[str],
) -> Dict[str, Any]:
    files: List[Dict[str, Any]] = []
    language_counts: Dict[str, int] = {}
    symbol_count = 0
    started = time.time()

    for path in iter_code_files(
        root,
        max_bytes=max_bytes,
        extra_skip_dirs=extra_skip_dirs,
        ignore_patterns=ignore_patterns,
    ):
        lang = detect_language(path)
        if lang is None:
            continue
        rel = path.relative_to(root).as_posix()
        parsed = parse_file(path, lang)
        entry = {
            "path": rel,
            "abs_path": str(path.resolve()),
            "lang": lang,
            "mtime": path.stat().st_mtime,
            **parsed,
        }
        files.append(entry)
        language_counts[lang] = language_counts.get(lang, 0) + 1
        symbol_count += len(entry.get("symbols", []))

    payload = {
        "version": VERSION,
        "root": str(root.resolve()),
        "created_at": int(time.time()),
        "duration_s": round(time.time() - started, 3),
        "max_bytes": max_bytes,
        "ignore_globs": list(ignore_patterns),
        "extra_skip_dirs": list(extra_skip_dirs),
        "files_indexed": len(files),
        "symbols_indexed": symbol_count,
        "language_counts": language_counts,
        "files": sorted(files, key=lambda item: item["path"]),
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(payload, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    return payload


def load_index(path: Path) -> Dict[str, Any]:
    if not path.exists():
        raise SystemExit(f"index not found: {path}")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"failed to parse index {path}: {exc}") from exc


def path_filter_match(path_value: str, path_contains: Optional[str], path_glob: Optional[str]) -> bool:
    path_l = path_value.lower()
    if path_contains and path_contains.lower() not in path_l:
        return False
    if path_glob and not path_matches_pattern(path_value, path_glob):
        return False
    return True


def nonzero_feature_scores(feature_scores: Dict[str, float]) -> Dict[str, float]:
    return {
        key: round(value, 3)
        for key, value in feature_scores.items()
        if abs(value) > 1e-9
    }


def summarize_feature_scores(feature_scores: Dict[str, float], limit: int = 3) -> str:
    pairs = [(name, value) for name, value in feature_scores.items() if value > 0]
    if not pairs:
        return ""
    pairs.sort(key=lambda item: (-item[1], item[0]))
    return ", ".join(f"{name}={value:.1f}" for name, value in pairs[:limit])


def path_terms(path_value: str) -> List[str]:
    return normalize_terms(path_value.replace("/", " "))


def best_fuzzy_path_match(term: str, candidates: Sequence[str]) -> Tuple[float, Optional[str]]:
    if len(term) < 4:
        return 0.0, None
    best_score = 0.0
    best_hit: Optional[str] = None
    for candidate in candidates:
        if len(candidate) < 3 or candidate == term:
            continue
        ratio = difflib.SequenceMatcher(None, term, candidate).ratio()
        if ratio >= 0.92 and 2.0 > best_score:
            best_score = 2.0
            best_hit = candidate
        elif ratio >= 0.84 and 1.2 > best_score:
            best_score = 1.2
            best_hit = candidate
    return best_score, best_hit


def score_file(
    entry: Dict[str, Any],
    query: str,
    terms: Sequence[str],
) -> Tuple[float, List[Dict[str, Any]], List[str], Dict[str, Any]]:
    matched_symbols: List[Dict[str, Any]] = []
    matched_lexemes: List[str] = []
    path_l = entry["path"].lower()
    path_terms_l = path_terms(path_l)
    imports_l = [str(x).lower() for x in entry.get("imports", [])]
    lexemes_l = [str(x).lower() for x in entry.get("lexemes", [])]
    covered_terms: set[str] = set()
    path_hits: List[str] = []
    import_hits: List[str] = []
    symbol_hits: List[str] = []
    is_pathish_query = "/" in query or "." in query or any(term in PATHISH_HINT_TERMS for term in terms)
    feature_scores: Dict[str, float] = {
        "path_exact_query": 0.0,
        "path_term_hits": 0.0,
        "path_component_exact": 0.0,
        "path_component_fuzzy": 0.0,
        "path_alignment_bonus": 0.0,
        "import_exact": 0.0,
        "import_partial": 0.0,
        "lexeme_exact": 0.0,
        "lexeme_partial": 0.0,
        "symbol_name_exact": 0.0,
        "symbol_qual_exact": 0.0,
        "symbol_partial": 0.0,
        "symbol_query": 0.0,
        "coverage_bonus": 0.0,
    }

    if query and query in path_l:
        feature_scores["path_exact_query"] += 6.0

    for term in terms:
        if term in path_l:
            feature_scores["path_term_hits"] += 3.0
            covered_terms.add(term)
            if term not in path_hits:
                path_hits.append(term)
        elif term in path_terms_l:
            feature_scores["path_component_exact"] += 2.0
            covered_terms.add(term)
            if term not in path_hits:
                path_hits.append(term)
        else:
            fuzzy_score, fuzzy_hit = best_fuzzy_path_match(term, path_terms_l)
            if fuzzy_score > 0 and fuzzy_hit is not None:
                feature_scores["path_component_fuzzy"] += fuzzy_score
                covered_terms.add(term)
                mapped = f"{term}->{fuzzy_hit}"
                if mapped not in path_hits:
                    path_hits.append(mapped)

        best_import_score = 0.0
        best_import_hit: Optional[str] = None
        for imp in imports_l:
            if term == imp:
                best_import_score = 3.0
                best_import_hit = imp
                break
            elif term in imp:
                if 1.5 > best_import_score:
                    best_import_score = 1.5
                    best_import_hit = imp
        if best_import_score > 0 and best_import_hit is not None:
            if best_import_score >= 3.0:
                feature_scores["import_exact"] += best_import_score
            else:
                feature_scores["import_partial"] += best_import_score
            covered_terms.add(term)
            if best_import_hit not in import_hits:
                import_hits.append(best_import_hit)

        best_lexeme_score = 0.0
        best_lexeme_hit: Optional[str] = None
        for lex in lexemes_l:
            if term == lex:
                best_lexeme_score = 1.5
                best_lexeme_hit = lex
                break
            elif term in lex:
                if 0.5 > best_lexeme_score:
                    best_lexeme_score = 0.5
                    best_lexeme_hit = lex
        if best_lexeme_score > 0 and best_lexeme_hit is not None:
            if best_lexeme_score >= 1.5:
                feature_scores["lexeme_exact"] += best_lexeme_score
            else:
                feature_scores["lexeme_partial"] += best_lexeme_score
            covered_terms.add(term)
            if best_lexeme_hit not in matched_lexemes:
                matched_lexemes.append(best_lexeme_hit)

    for symbol in entry.get("symbols", []):
        sym_blob = " ".join(
            str(symbol.get(k, "")) for k in ("name", "qualname", "kind")
        ).lower()
        sym_score = 0.0
        sym_features = {
            "symbol_name_exact": 0.0,
            "symbol_qual_exact": 0.0,
            "symbol_partial": 0.0,
            "symbol_query": 0.0,
        }
        if query and query in sym_blob:
            sym_score += 10.0
            sym_features["symbol_query"] += 10.0
        for term in terms:
            if term == str(symbol.get("name", "")).lower():
                sym_score += 10.0
                sym_features["symbol_name_exact"] += 10.0
                covered_terms.add(term)
            elif term == str(symbol.get("qualname", "")).lower():
                sym_score += 12.0
                sym_features["symbol_qual_exact"] += 12.0
                covered_terms.add(term)
            elif term in sym_blob:
                sym_score += 4.0
                sym_features["symbol_partial"] += 4.0
                covered_terms.add(term)
        if sym_score > 0:
            matched = dict(symbol)
            matched["_score"] = sym_score
            matched["_feature_scores"] = sym_features
            matched_symbols.append(matched)
            qn = str(symbol.get("qualname", ""))
            if qn and qn not in symbol_hits:
                symbol_hits.append(qn)

    matched_symbols.sort(key=lambda item: (-item["_score"], item["lineno"]))
    top_symbols = matched_symbols[:3]
    top_symbol_score = sum(item["_score"] for item in top_symbols)
    for item in top_symbols:
        for name, value in item.get("_feature_scores", {}).items():
            feature_scores[name] += float(value)
    if is_pathish_query and len(path_hits) >= 2:
        feature_scores["path_alignment_bonus"] = float(len(path_hits) * len(path_hits)) * 2.0
    coverage_bonus = float(len(covered_terms)) * 2.5
    feature_scores["coverage_bonus"] = coverage_bonus
    score = (
        feature_scores["path_exact_query"]
        + feature_scores["path_term_hits"]
        + feature_scores["path_component_exact"]
        + feature_scores["path_component_fuzzy"]
        + feature_scores["path_alignment_bonus"]
        + feature_scores["import_exact"]
        + feature_scores["import_partial"]
        + feature_scores["lexeme_exact"]
        + feature_scores["lexeme_partial"]
        + top_symbol_score
        + coverage_bonus
    )

    cleaned_symbols: List[Dict[str, Any]] = []
    for item in matched_symbols[:5]:
        cleaned = dict(item)
        cleaned.pop("_feature_scores", None)
        cleaned_symbols.append(cleaned)

    explain = {
        "terms": list(terms),
        "covered_terms": [term for term in terms if term in covered_terms],
        "missed_terms": [term for term in terms if term not in covered_terms],
        "coverage_ratio": round((len(covered_terms) / float(len(terms))) if terms else 0.0, 3),
        "feature_scores": nonzero_feature_scores(feature_scores),
        "path_hits": path_hits[:8],
        "import_hits": import_hits[:8],
        "lexeme_hits": matched_lexemes[:8],
        "symbol_hits": symbol_hits[:5],
    }
    return score, cleaned_symbols, matched_lexemes[:8], explain


def annotate_confidence(rows: List[Dict[str, Any]]) -> List[Dict[str, Any]]:
    annotated: List[Dict[str, Any]] = []
    for idx, row in enumerate(rows, start=1):
        next_score = float(rows[idx]["score"]) if idx < len(rows) else 0.0
        score = float(row["score"])
        margin = max(0.0, score - next_score)
        margin_ratio = margin / max(score, 1.0)
        explain = row["explain"]
        feature_scores = explain.get("feature_scores", {})
        coverage_ratio = float(explain.get("coverage_ratio", 0.0))

        exact_signal = 0.0
        if feature_scores.get("symbol_qual_exact", 0.0) > 0 or feature_scores.get("symbol_name_exact", 0.0) > 0:
            exact_signal += 0.18
        elif feature_scores.get("symbol_query", 0.0) > 0 or feature_scores.get("symbol_partial", 0.0) > 0:
            exact_signal += 0.1
        if feature_scores.get("path_exact_query", 0.0) > 0:
            exact_signal += 0.08
        elif feature_scores.get("path_component_exact", 0.0) > 0:
            exact_signal += 0.05
        if feature_scores.get("path_alignment_bonus", 0.0) > 0:
            exact_signal += 0.08
        if feature_scores.get("import_exact", 0.0) > 0:
            exact_signal += 0.04

        confidence = clamp01(
            0.2
            + min(0.45, coverage_ratio * 0.45)
            + min(0.25, margin_ratio * 0.6)
            + min(0.2, exact_signal)
        )

        updated = dict(row)
        updated["rank"] = idx
        updated["margin_to_next"] = round(margin, 3)
        updated["margin_ratio"] = round(margin_ratio, 3)
        updated["confidence"] = round(confidence, 3)
        updated["confidence_label"] = confidence_label(confidence)
        annotated.append(updated)
    return annotated


def query_index(
    index: Dict[str, Any],
    query: str,
    *,
    top_k: int,
    lang: Optional[str] = None,
    path_contains: Optional[str] = None,
    path_glob: Optional[str] = None,
) -> List[Dict[str, Any]]:
    query_l = query.strip().lower()
    terms = normalize_terms(query_l)
    rows: List[Dict[str, Any]] = []
    for entry in index.get("files", []):
        if lang and str(entry.get("lang")) != lang:
            continue
        if not path_filter_match(entry["path"], path_contains=path_contains, path_glob=path_glob):
            continue
        score, matched_symbols, matched_lexemes, explain = score_file(entry, query_l, terms)
        if score <= 0:
            continue
        rows.append(
            {
                "score": round(score, 3),
                "entry": entry,
                "symbols": matched_symbols,
                "lexemes": matched_lexemes,
                "explain": explain,
            }
        )

    rows.sort(key=lambda item: (-float(item["score"]), item["entry"]["path"]))
    return annotate_confidence(rows[:top_k])


def cmd_build(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    settings, _config_path = resolve_repo_settings(root, args.config)
    out_path = resolve_index_path(root, settings, args.index or args.out)
    ignore_patterns = load_ignore_patterns(root, settings, args.ignore_file)
    extra_skip_dirs = list(settings.get("scan", {}).get("skip_dirs", []))
    max_bytes = int(args.max_bytes or settings.get("scan", {}).get("max_bytes", DEFAULT_MAX_BYTES))
    payload = build_index(
        root,
        out_path,
        max_bytes=max_bytes,
        extra_skip_dirs=extra_skip_dirs,
        ignore_patterns=ignore_patterns,
    )
    print(
        json.dumps(
            {
                "ok": True,
                "index": str(out_path),
                "root": payload["root"],
                "files": payload["files_indexed"],
                "symbols": payload["symbols_indexed"],
                "duration_s": payload["duration_s"],
                "languages": payload["language_counts"],
            },
            ensure_ascii=False,
        )
    )
    return 0


def cmd_query(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    settings, _config_path = resolve_repo_settings(root, args.config)
    index = load_index(resolve_index_path(root, settings, args.index))
    rows = query_index(
        index,
        args.query,
        top_k=int(args.top_k or settings.get("query", {}).get("top_k", 10)),
        lang=args.lang,
        path_contains=args.path_contains,
        path_glob=args.path_glob,
    )

    if args.json:
        print(
            json.dumps(
                [
                    {
                        "rank": row["rank"],
                        "score": row["score"],
                        "confidence": row["confidence"],
                        "confidence_label": row["confidence_label"],
                        "margin_to_next": row["margin_to_next"],
                        "margin_ratio": row["margin_ratio"],
                        "path": row["entry"]["path"],
                        "lang": row["entry"]["lang"],
                        "symbols": row["symbols"],
                        "lexemes": row["lexemes"],
                        "imports": row["entry"].get("imports", []),
                        **({"explain": row["explain"]} if args.explain else {}),
                    }
                    for row in rows
                ],
                ensure_ascii=False,
                indent=2,
            )
        )
        return 0

    if not rows:
        print("no matches")
        return 0

    for row in rows:
        entry = row["entry"]
        matched_symbols = row["symbols"]
        matched_lexemes = row["lexemes"]
        print(
            f"{row['rank']:02d}. score={row['score']:.1f} conf={row['confidence']:.2f} "
            f"margin={row['margin_to_next']:.1f}  {entry['path']}  [{entry['lang']}]"
        )
        for symbol in matched_symbols[:3]:
            print(
                f"    - {symbol['kind']} {symbol['qualname']} "
                f"@{symbol['lineno']}:{symbol['end_lineno']} score={symbol['_score']:.1f}"
            )
        if not matched_symbols and matched_lexemes:
            print(f"    - lexemes: {', '.join(matched_lexemes[:5])}")
        elif not matched_symbols and entry.get("imports"):
            shown = ", ".join(entry["imports"][:5])
            if shown:
                print(f"    - imports: {shown}")
        if args.explain:
            explain = row["explain"]
            features = summarize_feature_scores(explain.get("feature_scores", {}), limit=4)
            if features:
                print(f"    - explain: {features}")
            covered = explain.get("covered_terms", [])
            missed = explain.get("missed_terms", [])
            if covered:
                print(f"    - covered: {', '.join(covered[:8])}")
            if explain.get("path_hits"):
                print(f"    - path_hits: {', '.join(explain['path_hits'][:8])}")
            if explain.get("import_hits"):
                print(f"    - import_hits: {', '.join(explain['import_hits'][:8])}")
            if explain.get("lexeme_hits"):
                print(f"    - lexeme_hits: {', '.join(explain['lexeme_hits'][:8])}")
            if explain.get("symbol_hits"):
                print(f"    - symbol_hits: {', '.join(explain['symbol_hits'][:5])}")
            if missed:
                print(f"    - missed: {', '.join(missed[:8])}")
    return 0


def resolve_file(index: Dict[str, Any], target: str) -> Dict[str, Any]:
    target_norm = target.replace("\\", "/")
    files = index.get("files", [])
    exact = [f for f in files if f["path"] == target_norm or f["abs_path"] == target_norm]
    if len(exact) == 1:
        return exact[0]
    base = [f for f in files if Path(f["path"]).name == Path(target_norm).name]
    if len(base) == 1:
        return base[0]
    suffix = [f for f in files if f["path"].endswith(target_norm)]
    if len(suffix) == 1:
        return suffix[0]
    candidates = exact or base or suffix
    if not candidates:
        raise SystemExit(f"file not found in index: {target}")
    shown = ", ".join(f["path"] for f in candidates[:8])
    raise SystemExit(f"ambiguous file target: {target} -> {shown}")


def find_symbol(
    index: Dict[str, Any],
    symbol_name: str,
    file_entry: Optional[Dict[str, Any]] = None,
) -> Tuple[Dict[str, Any], Dict[str, Any]]:
    candidates: List[Tuple[Dict[str, Any], Dict[str, Any]]] = []
    files = [file_entry] if file_entry is not None else index.get("files", [])
    sym_l = symbol_name.lower()
    for entry in files:
        if entry is None:
            continue
        for symbol in entry.get("symbols", []):
            name = str(symbol.get("name", "")).lower()
            qualname = str(symbol.get("qualname", "")).lower()
            if sym_l == name or sym_l == qualname or qualname.endswith("." + sym_l):
                candidates.append((entry, symbol))

    if len(candidates) == 1:
        return candidates[0]
    if not candidates:
        raise SystemExit(f"symbol not found in index: {symbol_name}")
    shown = ", ".join(
        f"{entry['path']}::{symbol['qualname']}" for entry, symbol in candidates[:8]
    )
    raise SystemExit(f"ambiguous symbol target: {symbol_name} -> {shown}")


def read_lines(path: Path, start: int, end: int, raw: bool) -> None:
    text = path.read_text(encoding="utf-8", errors="ignore").splitlines()
    start = max(1, start)
    end = min(len(text), end)
    width = len(str(end))
    for lineno in range(start, end + 1):
        line = text[lineno - 1]
        if raw:
            print(line)
        else:
            print(f"{lineno:>{width}} | {line}")


def parse_line_range(spec: str) -> Tuple[int, int]:
    match = re.fullmatch(r"(\d+)\s*:\s*(\d+)", spec)
    if not match:
        raise SystemExit(f"invalid line range, expected START:END but got: {spec}")
    start, end = int(match.group(1)), int(match.group(2))
    if end < start:
        raise SystemExit(f"invalid line range, end < start: {spec}")
    return start, end


def cmd_show(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    settings, _config_path = resolve_repo_settings(root, args.config)
    index = load_index(resolve_index_path(root, settings, args.index))
    file_entry: Optional[Dict[str, Any]] = None
    symbol: Optional[Dict[str, Any]] = None

    if args.file:
        file_entry = resolve_file(index, args.file)
    if args.symbol:
        file_entry, symbol = find_symbol(index, args.symbol, file_entry=file_entry)
    if file_entry is None:
        raise SystemExit("show requires --file or --symbol")

    start = 1
    end = file_entry.get("line_count", 1)
    if args.lines:
        start, end = parse_line_range(args.lines)
    elif symbol is not None:
        start = int(symbol["lineno"])
        end = int(symbol["end_lineno"])

    start = max(1, start - args.context)
    end = end + args.context

    if not args.raw:
        print(f"# {file_entry['path']}")
        if symbol is not None:
            print(f"# symbol: {symbol['qualname']} [{symbol['kind']}] {symbol['lineno']}:{symbol['end_lineno']}")
        else:
            print(f"# lines: {start}:{end}")
    read_lines(Path(file_entry["abs_path"]), start, end, raw=args.raw)
    return 0


def matches_expected_path(found_path: str, expected: str) -> bool:
    found_norm = found_path.replace("\\", "/")
    expected_norm = expected.replace("\\", "/")
    return (
        found_norm == expected_norm
        or found_norm.endswith(expected_norm)
        or Path(found_norm).name == Path(expected_norm).name
    )


def matches_expected(found_path: str, expected_paths: Sequence[str], expect_globs: Sequence[str]) -> bool:
    if any(matches_expected_path(found_path, expected) for expected in expected_paths):
        return True
    return any(path_matches_pattern(found_path, pattern) for pattern in expect_globs)


def load_eval_cases(path: Path) -> List[Dict[str, Any]]:
    payload = load_json(path)
    cases = payload.get("cases", [])
    if not isinstance(cases, list):
        raise SystemExit(f"expected 'cases' array in {path}")
    return [case for case in cases if isinstance(case, dict)]


def cmd_eval(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    settings, _config_path = resolve_repo_settings(root, args.config)
    index = load_index(resolve_index_path(root, settings, args.index))
    eval_path = resolve_path(root, args.eval_file or settings.get("eval_file") or DEFAULT_EVAL)
    cases = load_eval_cases(eval_path)
    if not cases:
        print("no eval cases")
        return 0

    passed = 0
    reciprocal_ranks = 0.0
    confidence_sum = 0.0
    margin_sum = 0.0
    failures: List[str] = []

    for case in cases:
        name = str(case.get("name") or case.get("query") or "unnamed")
        query = str(case.get("query") or "").strip()
        expected = [str(x) for x in case.get("expect_paths", [])]
        expect_globs = [str(x) for x in case.get("expect_globs", [])]
        reject_paths = [str(x) for x in case.get("reject_paths", [])]
        reject_globs = [str(x) for x in case.get("reject_globs", [])]
        top_k = int(case.get("top_k", args.top_k or settings.get("query", {}).get("top_k", 10)))
        max_rank = int(case.get("max_rank", top_k))
        min_confidence = float(case.get("min_confidence", 0.0))
        min_margin = float(case.get("min_margin", 0.0))
        require_first = bool(case.get("require_first", False))
        rows = query_index(
            index,
            query,
            top_k=top_k,
            lang=case.get("lang"),
            path_contains=case.get("path_contains"),
            path_glob=case.get("path_glob"),
        )
        rank: Optional[int] = None
        matched_row: Optional[Dict[str, Any]] = None
        rejected_hits: List[Dict[str, Any]] = []
        for row in rows:
            if matches_expected(row["entry"]["path"], reject_paths, reject_globs):
                rejected_hits.append(row)
            if matches_expected(row["entry"]["path"], expected, expect_globs):
                rank = int(row["rank"])
                matched_row = row
                break
        reasons: List[str] = []
        if rank is None:
            reasons.append("expected path not found")
        else:
            if rank > max_rank:
                reasons.append(f"rank {rank} > max_rank {max_rank}")
            if require_first and rank != 1:
                reasons.append(f"expected rank 1 but got {rank}")
        if matched_row is not None:
            if float(matched_row["confidence"]) < min_confidence:
                reasons.append(
                    f"confidence {matched_row['confidence']:.3f} < min_confidence {min_confidence:.3f}"
                )
            if float(matched_row["margin_to_next"]) < min_margin:
                reasons.append(
                    f"margin {matched_row['margin_to_next']:.3f} < min_margin {min_margin:.3f}"
                )
        if rejected_hits:
            reasons.append(f"rejected path present: {rejected_hits[0]['entry']['path']}")
        ok = not reasons
        if ok:
            passed += 1
            reciprocal_ranks += 1.0 / float(rank)
            if matched_row is not None:
                confidence_sum += float(matched_row["confidence"])
                margin_sum += float(matched_row["margin_to_next"])
                print(
                    f"PASS  {name}  rank={rank}  conf={matched_row['confidence']:.3f} "
                    f"margin={matched_row['margin_to_next']:.3f}  query={query}"
                )
            else:
                print(f"PASS  {name}  rank={rank}  query={query}")
        else:
            failures.append(name)
            print(f"FAIL  {name}  query={query}")
            print(f"  reasons: {'; '.join(reasons)}")
            for row in rows[:3]:
                tail = summarize_feature_scores(row["explain"].get("feature_scores", {}), limit=3)
                symbol_tail = ""
                if row["symbols"]:
                    symbol_tail = f"{row['symbols'][0]['qualname']}@{row['symbols'][0]['lineno']}"
                elif row["lexemes"]:
                    symbol_tail = ",".join(row["lexemes"][:3])
                print(
                    f"  {row['rank']}. score={row['score']:.1f} conf={row['confidence']:.3f} "
                    f"margin={row['margin_to_next']:.3f} {row['entry']['path']} "
                    f"{symbol_tail} {tail}".rstrip()
                )

    total = len(cases)
    print(
        json.dumps(
            {
                "ok": True,
                "cases": total,
                "passed": passed,
                "pass_rate": round((passed / total) if total else 0.0, 4),
                "mrr": round((reciprocal_ranks / total) if total else 0.0, 4),
                "avg_confidence": round((confidence_sum / passed) if passed else 0.0, 4),
                "avg_margin": round((margin_sum / passed) if passed else 0.0, 4),
                "failures": failures,
                "eval_file": str(eval_path),
            },
            ensure_ascii=False,
        )
    )
    return 0 if passed == total else 1


def build_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(description="Repository map v1")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_build = sub.add_parser("build", help="scan repo and write repo map cache")
    p_build.add_argument("--root", default=".", help="repository root (default: cwd)")
    p_build.add_argument("--config", help=f"config path (default: {DEFAULT_CONFIG} if present)")
    p_build.add_argument("--ignore-file", help=f"ignore file (default: {DEFAULT_IGNORE} or config)")
    p_build.add_argument("--index", help=f"index path (default: {DEFAULT_INDEX} or config)")
    p_build.add_argument("--out", help="legacy alias for --index")
    p_build.add_argument("--max-bytes", type=int, help="skip files larger than this size")
    p_build.set_defaults(func=cmd_build)

    p_query = sub.add_parser("query", help="rank likely files/symbols for a query")
    p_query.add_argument("--root", default=".", help="repository root (default: cwd)")
    p_query.add_argument("--config", help=f"config path (default: {DEFAULT_CONFIG} if present)")
    p_query.add_argument("query", help="free-text query")
    p_query.add_argument("--index", help=f"index path (default: {DEFAULT_INDEX} or config)")
    p_query.add_argument("--top-k", type=int, help="number of matches to print")
    p_query.add_argument("--lang", choices=sorted(set(LANG_BY_EXT.values())), help="language filter")
    p_query.add_argument("--path-contains", help="substring that must appear in the path")
    p_query.add_argument("--path-glob", help="glob that the path must match")
    p_query.add_argument("--json", action="store_true", help="emit JSON")
    p_query.add_argument("--explain", action="store_true", help="include score breakdown and hit details")
    p_query.set_defaults(func=cmd_query)

    p_show = sub.add_parser("show", help="print only a symbol span or line range")
    p_show.add_argument("--root", default=".", help="repository root (default: cwd)")
    p_show.add_argument("--config", help=f"config path (default: {DEFAULT_CONFIG} if present)")
    p_show.add_argument("--index", help=f"index path (default: {DEFAULT_INDEX} or config)")
    p_show.add_argument("--file", help="target file path or basename")
    p_show.add_argument("--symbol", help="symbol name or qualname")
    p_show.add_argument("--lines", help="explicit line range START:END")
    p_show.add_argument("--context", type=int, default=2, help="extra context lines before/after selection")
    p_show.add_argument("--raw", action="store_true", help="print raw lines without headers or line numbers")
    p_show.set_defaults(func=cmd_show)

    p_eval = sub.add_parser("eval", help="run query benchmark cases")
    p_eval.add_argument("--root", default=".", help="repository root (default: cwd)")
    p_eval.add_argument("--config", help=f"config path (default: {DEFAULT_CONFIG} if present)")
    p_eval.add_argument("--index", help=f"index path (default: {DEFAULT_INDEX} or config)")
    p_eval.add_argument("--eval-file", help=f"eval case file (default: {DEFAULT_EVAL} or config)")
    p_eval.add_argument("--top-k", type=int, help="fallback top-k when a case omits it")
    p_eval.set_defaults(func=cmd_eval)

    return ap


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return int(args.func(args))


if __name__ == "__main__":
    sys.exit(main())
