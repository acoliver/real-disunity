#!/usr/bin/env python3
"""Change classification and documentation checks for the PR workflow.

Subcommands:

  classify        Decide whether a diff touches only Markdown documents.
  check-docs      Verify relative links and anchors in added or modified Markdown files.
  markdown-inputs Fail when a code build read a Markdown file, which would make the
                  docs-only shortcut unsound.

Standard library only. The workflow runs the classifier from the base commit so a pull
request cannot change the rule that decides whether its own code is built.
"""

import argparse
import os
import posixpath
import re
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Callable, Iterable, NamedTuple
from urllib.parse import unquote

REGULAR = "100644"
ABSENT = "000000"
EXPECTED_MODES = {"A": (ABSENT, REGULAR), "M": (REGULAR, REGULAR), "D": (REGULAR, ABSENT)}
HEADER = re.compile(r"^:(\d{6}) (\d{6}) ([0-9a-f]{40,64}) ([0-9a-f]{40,64}) ([A-Z])(\d*)$")
REPO_URL = re.compile(r"^https://github\.com/acoliver/real-disunity/(?:blob|tree)/main/(.*)$")
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


class Change(NamedTuple):
    status: str
    old_mode: str
    new_mode: str
    path: str


class ParseError(Exception):
    pass


def parse_raw(output: str) -> list[Change]:
    """Parses `git diff --raw -z --no-renames` output."""
    if output == "":
        return []
    if not output.endswith("\0"):
        raise ParseError("raw diff output is not NUL terminated")
    tokens = output[:-1].split("\0")
    if len(tokens) % 2 != 0:
        raise ParseError("raw diff output has a header without a path")
    changes = []
    for header, path in zip(tokens[0::2], tokens[1::2]):
        match = HEADER.match(header)
        if match is None or path == "":
            raise ParseError(f"unrecognised raw diff record: {header!r} {path!r}")
        status = match.group(5)
        if status in ("R", "C"):
            raise ParseError("rename or copy record where renames are disabled")
        changes.append(Change(status, match.group(1), match.group(2), path))
    return changes


def is_markdown_path(path: str) -> bool:
    name = posixpath.basename(path)
    return name.endswith(".md") and name != ".md"


def non_documentation_reason(change: Change) -> str | None:
    """Returns why a change is not a plain Markdown edit, or None when it is one."""
    modes = EXPECTED_MODES.get(change.status)
    if modes is None:
        return f"{change.path}: status {change.status} is not an add, modify or delete"
    if (change.old_mode, change.new_mode) != modes:
        return f"{change.path}: modes {change.old_mode}->{change.new_mode} are not a regular file"
    if not is_markdown_path(change.path):
        return f"{change.path}: not a Markdown file"
    return None


def classify_changes(changes: list[Change]) -> tuple[bool, str]:
    if not changes:
        return False, "empty diff"
    for change in changes:
        reason = non_documentation_reason(change)
        if reason is not None:
            return False, reason
    return True, f"{len(changes)} Markdown file change(s) only"


def git_raw_diff(base: str, head: str) -> str:
    command = [
        "git", "diff", "--raw", "-z", "--no-renames", "--no-abbrev", "--no-color",
        "--ignore-submodules=none", base, head,
    ]
    result = subprocess.run(command, capture_output=True, check=True)
    return result.stdout.decode("utf-8", errors="surrogateescape")


def classify(base: str, head: str, diff: Callable[[str, str], str] = git_raw_diff) -> tuple[bool, str]:
    """Fails closed: any problem reading the diff means code may have changed."""
    try:
        return classify_changes(parse_raw(diff(base, head)))
    except (ParseError, OSError, subprocess.CalledProcessError) as error:
        return False, f"classification failed: {error}"


def append_to_file(variable: str, text: str) -> None:
    path = os.environ.get(variable)
    if path:
        with open(path, "a", encoding="utf-8") as handle:
            handle.write(text)


def command_classify(args: argparse.Namespace) -> int:
    try:
        docs_only, reason = classify(args.base, args.head)
    except Exception as error:  # noqa: BLE001 - the gate must still run and decide
        docs_only, reason = False, f"unexpected failure: {error}"
    print(f"docs_only={'true' if docs_only else 'false'} ({reason})")
    append_to_file("GITHUB_OUTPUT", f"docs_only={'true' if docs_only else 'false'}\n")
    append_to_file("GITHUB_STEP_SUMMARY", f"docs_only: {docs_only} ({reason})\n")
    return 0


# Link checking

def heading_slugs(lines: Iterable[str]) -> set[str]:
    """GitHub anchors for ATX and setext headings plus explicit HTML anchors."""
    slugs: set[str] = set()
    seen: dict[str, int] = {}
    for text in headings(list(lines)):
        base = github_slug(text)
        count = seen.get(base, 0)
        seen[base] = count + 1
        slugs.add(base if count == 0 else f"{base}-{count}")
    return slugs


def github_slug(text: str) -> str:
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text).lower()
    text = re.sub(r"[^\w \-]", "", text)
    return text.replace(" ", "-")


def strip_fences(lines: list[str]) -> list[tuple[int, str]]:
    """Returns (line number, text) for lines outside fenced code blocks."""
    kept = []
    fence = None
    for number, line in enumerate(lines, start=1):
        marker = re.match(r"^ {0,3}(`{3,}|~{3,})", line)
        if fence is None and marker:
            fence = marker.group(1)[0] * len(marker.group(1))
        elif fence is not None and re.match(rf"^ {{0,3}}{re.escape(fence[0])}{{{len(fence)},}}\s*$", line):
            fence = None
        elif fence is None:
            kept.append((number, line))
    return kept


def headings(lines: list[str]) -> list[str]:
    found = []
    visible = strip_fences(lines)
    for index, (_, line) in enumerate(visible):
        atx = re.match(r"^ {0,3}#{1,6}[ \t]+(.*?)(?:[ \t]+#+)?[ \t]*$", line)
        if atx:
            found.append(atx.group(1))
            continue
        following = visible[index + 1][1] if index + 1 < len(visible) else ""
        plain = line.strip() and not re.match(r"^\s*([#>*+\-]|\d+[.)]|\||<)", line)
        if plain and re.match(r"^ {0,3}(=+|-+)\s*$", following):
            found.append(line.strip())
    return found


def html_anchors(text: str) -> set[str]:
    found = re.findall(r"""<a\s+[^>]*?(?:name|id)=["']([^"']+)["']""", text)
    return {name.lower() for name in found}


def extract_links(lines: list[str]) -> list[tuple[int, str]]:
    links = []
    for number, line in strip_fences(lines):
        line = re.sub(r"`[^`]*`", "", line)
        for match in re.finditer(r"\]\(\s*(<[^>]*>|[^)\s]*)", line):
            links.append((number, match.group(1).strip("<>")))
        reference = re.match(r"^ {0,3}\[(?!\^)[^\]]+\]:\s*(\S+)", line)
        if reference:
            links.append((number, reference.group(1).strip("<>")))
    return [(number, target) for number, target in links if target]


class LinkChecker:
    def __init__(self, root: Path):
        self.root = root
        self.anchors: dict[Path, set[str]] = {}

    def anchors_of(self, path: Path) -> set[str]:
        if path not in self.anchors:
            text = path.read_text(encoding="utf-8", errors="replace")
            self.anchors[path] = heading_slugs(text.splitlines()) | html_anchors(text)
        return self.anchors[path]

    def resolve(self, source: str, target: str) -> tuple[str, str] | str | None:
        """Returns (repo-relative path, fragment), an error message, or None to skip."""
        repo = REPO_URL.match(target)
        if repo:
            target = repo.group(1)
            relative_to = ""
        elif SCHEME.match(target):
            return None
        else:
            relative_to = posixpath.dirname(source)
        path, _, fragment = target.partition("#")
        path = unquote(path.partition("?")[0])
        if path.startswith("/"):
            joined = path.lstrip("/")
        else:
            joined = posixpath.join(relative_to, path)
        normal = posixpath.normpath(joined) if joined else source
        if normal == ".." or normal.startswith("../"):
            return f"link leaves the repository: {target}"
        return (normal if path else source, unquote(fragment))

    def check_link(self, source: str, target: str) -> str | None:
        resolved = self.resolve(source, target)
        if resolved is None or isinstance(resolved, str):
            return resolved
        path, fragment = resolved
        local = self.root / path
        if not local.exists():
            return f"missing target {path}"
        if fragment and local.is_file() and path.endswith(".md"):
            if fragment.lower() not in self.anchors_of(local):
                return f"no heading or anchor #{fragment} in {path}"
        return None

    def check_file(self, source: str) -> list[str]:
        text = (self.root / source).read_text(encoding="utf-8", errors="replace")
        failures = []
        for number, target in extract_links(text.splitlines()):
            problem = self.check_link(source, target)
            if problem:
                failures.append(f"{source}:{number}: {problem} (link {target})")
        return failures


def check_docs(root: Path, changes: list[Change]) -> list[str]:
    checker = LinkChecker(root)
    failures = []
    for change in changes:
        if change.status in ("A", "M") and is_markdown_path(change.path):
            failures.extend(checker.check_file(change.path))
    return failures


def command_check_docs(args: argparse.Namespace) -> int:
    changes = parse_raw(git_raw_diff(args.base, args.head))
    failures = check_docs(Path(args.root), changes)
    for failure in failures:
        print(failure)
    if failures:
        print(f"{len(failures)} broken documentation link(s)")
        return 1
    print("documentation links resolve")
    return 0


# Markdown as a build input

def dep_info_paths(text: str) -> list[str]:
    """Paths named in a Makefile-style dep-info file, with `\\ ` escapes undone."""
    text = text.replace("\\\n", " ")
    tokens = re.findall(r"(?:\\ |\S)+", text)
    return [token.replace("\\ ", " ").rstrip(":") for token in tokens]


def build_output_paths(text: str) -> list[str]:
    paths = []
    for line in text.splitlines():
        match = re.match(r"^cargo::?rerun-if-changed=(.*)$", line)
        if match:
            paths.append(match.group(1))
    return paths


def workspace_packages(root: Path) -> set[str]:
    """Package names declared by `crates/*/Cargo.toml`, the workspace member glob."""
    names = set()
    for manifest in sorted((root / "crates").glob("*/Cargo.toml")):
        with open(manifest, "rb") as handle:
            name = tomllib.load(handle).get("package", {}).get("name")
        if name:
            names.add(name)
    return names


def build_package(directory: Path) -> str:
    """Package name of a build script run directory such as `blake3-0123456789abcdef`."""
    match = re.match(r"^(.+)-[0-9a-f]{16}$", directory.name)
    return match.group(1) if match else directory.name


def repo_markdown(paths: Iterable[str], root: Path, target: Path, relative_is_repo: bool = True) -> list[str]:
    """Markdown paths that point into the repository.

    Relative paths belong to the repository only when `relative_is_repo` says so: dep-info
    paths are relative to the workspace root, and a build script's paths are relative to its
    own package, which is in the repository only for workspace members.
    """
    found = []
    for path in paths:
        if not path.endswith(".md"):
            continue
        absolute = Path(path)
        if not absolute.is_absolute():
            if relative_is_repo:
                found.append(path)
        elif absolute.is_relative_to(root) and not absolute.is_relative_to(target):
            found.append(path)
    return found


def cargo_markdown_inputs(target: Path, root: Path) -> list[str]:
    failures = []
    for path in sorted(target.rglob("*.d")):
        text = path.read_text(encoding="utf-8", errors="replace")
        for hit in repo_markdown(dep_info_paths(text), root, target):
            failures.append(f"{path}: dep-info lists {hit}")
    members = workspace_packages(root)
    for path in sorted(target.rglob("output")):
        if path.is_file() and path.parent.parent.name == "build":
            text = path.read_text(encoding="utf-8", errors="replace")
            local = build_package(path.parent) in members
            for hit in repo_markdown(build_output_paths(text), root, target, local):
                failures.append(f"{path}: build script watches {hit}")
    return list(dict.fromkeys(failures))


def managed_markdown_inputs(root: Path) -> list[str]:
    failures = []
    for pattern in ("*.csproj", "*.props", "*.targets"):
        for path in sorted((root / "csharp").rglob(pattern)):
            if {"bin", "obj"} & set(path.relative_to(root).parts):
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            if re.search(r"\.md\b", text, re.IGNORECASE):
                failures.append(f"{path}: mentions a Markdown file")
    return failures


def command_markdown_inputs(args: argparse.Namespace) -> int:
    target = Path(args.target).resolve()
    root = Path(args.root).resolve()
    if not target.is_dir() or next(target.rglob("*.d"), None) is None:
        print(f"{target} holds no cargo dep-info; the build output cannot be checked")
        return 1
    failures = cargo_markdown_inputs(target, root) + managed_markdown_inputs(root)
    for failure in failures:
        print(failure)
    if failures:
        print("Markdown is a build input, so documentation-only PRs cannot skip the build")
        return 1
    print("no Markdown file is a build input")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name, handler in (("classify", command_classify), ("check-docs", command_check_docs)):
        sub = commands.add_parser(name)
        sub.add_argument("--base", required=True)
        sub.add_argument("--head", required=True)
        sub.add_argument("--root", default=".")
        sub.set_defaults(handler=handler)
    inputs = commands.add_parser("markdown-inputs")
    inputs.add_argument("--target", required=True)
    inputs.add_argument("--root", required=True)
    inputs.set_defaults(handler=command_markdown_inputs)
    return parser


def main(argv: list[str]) -> int:
    args = build_parser().parse_args(argv)
    return args.handler(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
