import contextlib
import io
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

import ci_changes as cc

SHA_A = "a" * 40
SHA_B = "b" * 40
ZERO = "0" * 40


def record(old, new, status, path):
    return f":{old} {new} {SHA_A} {SHA_B} {status}\0{path}\0"


def added(path):
    return record("000000", "100644", "A", path)


def modified(path):
    return record("100644", "100644", "M", path)


def deleted(path):
    return record("100644", "000000", "D", path)


def classify_raw(raw):
    return cc.classify("base", "head", lambda *_: raw)


class ClassifyRecords(unittest.TestCase):
    def test_markdown_additions_modifications_and_deletions_are_docs_only(self):
        raw = added("docs/new.md") + modified("README.md") + deleted("old/notes.md")
        self.assertTrue(classify_raw(raw)[0])

    def test_empty_diff_is_not_docs_only(self):
        self.assertFalse(classify_raw("")[0])

    def test_code_alongside_markdown_is_not_docs_only(self):
        raw = modified("README.md") + modified("crates/play/src/main.rs")
        self.assertFalse(classify_raw(raw)[0])

    def test_workflow_and_script_changes_are_code(self):
        for path in (".github/workflows/pr.yml", ".github/scripts/ci_changes.py", "Cargo.lock"):
            self.assertFalse(classify_raw(modified(path))[0], path)

    def test_extension_must_be_exactly_lowercase_md(self):
        for path in ("README.MD", "notes.markdown", "notes.md.txt", "md", "dir/.md", "a.mdx"):
            self.assertFalse(classify_raw(modified(path))[0], path)

    def test_rename_of_code_to_markdown_shows_as_delete_plus_add_and_is_code(self):
        raw = deleted("crates/play/src/main.rs") + added("docs/main.md")
        self.assertFalse(classify_raw(raw)[0])

    def test_rename_between_markdown_files_is_docs_only(self):
        raw = deleted("docs/a.md") + added("docs/b.md")
        self.assertTrue(classify_raw(raw)[0])

    def test_special_modes_are_not_regular_markdown(self):
        cases = [
            record("000000", "120000", "A", "link.md"),
            record("120000", "120000", "M", "link.md"),
            record("000000", "160000", "A", "sub.md"),
            record("100644", "100755", "M", "script.md"),
            record("100755", "100755", "M", "script.md"),
            record("100644", "120000", "T", "typechange.md"),
            record("100644", "100644", "T", "typechange.md"),
            record("100644", "100644", "U", "conflict.md"),
        ]
        for raw in cases:
            self.assertFalse(classify_raw(raw)[0], raw)

    def test_status_must_agree_with_modes(self):
        self.assertFalse(classify_raw(record("100644", "000000", "A", "a.md"))[0])
        self.assertFalse(classify_raw(record("000000", "100644", "D", "a.md"))[0])
        self.assertFalse(classify_raw(record("000000", "100644", "M", "a.md"))[0])

    def test_malformed_output_is_not_docs_only(self):
        for raw in (
            "garbage",
            ":100644 100644 x y M\0a.md\0",
            added("a.md").rstrip("\0"),
            ":100644 100644 " + SHA_A + " " + SHA_B + " M\0",
            record("100644", "100644", "R", "a.md"),
            added(""),
        ):
            self.assertFalse(classify_raw(raw)[0], raw)

    def test_git_failure_is_not_docs_only(self):
        def fail(*_):
            raise subprocess.CalledProcessError(128, "git")

        self.assertFalse(cc.classify("a", "b", fail)[0])


def scope_raw(raw):
    return cc.classify_scope("base", "head", lambda *_: raw)[0]


class Scopes(unittest.TestCase):
    def test_markdown_only_is_docs(self):
        self.assertEqual(scope_raw(modified("README.md") + added("docs/new.md")), "docs")

    def test_csharp_changes_with_or_without_markdown_are_csharp(self):
        self.assertEqual(scope_raw(modified("csharp/Disunity/Material.cs")), "csharp")
        raw = added("csharp/Disunity/Graphics/Texture.cs") + deleted("csharp/Disunity.Tests/Old.cs")
        self.assertEqual(scope_raw(raw + modified("docs/unity-api.md")), "csharp")
        self.assertEqual(scope_raw(modified("csharp/Disunity/Disunity.Engine.csproj")), "csharp")

    def test_csharp_with_rust_or_other_files_is_full(self):
        self.assertEqual(scope_raw(modified("csharp/Disunity/Material.cs") + modified("crates/play/src/keys.rs")), "full")
        self.assertEqual(scope_raw(modified("csharp/Disunity/Material.cs") + modified("Cargo.lock")), "full")
        self.assertEqual(scope_raw(modified("csharp/Disunity/Material.cs") + modified(".github/workflows/pr.yml")), "full")

    def test_csharp_prefix_must_be_the_top_level_directory(self):
        self.assertEqual(scope_raw(modified("crates/csharp/src/lib.rs")), "full")
        self.assertEqual(scope_raw(modified("csharpx/File.cs")), "full")

    def test_special_modes_under_csharp_are_full(self):
        self.assertEqual(scope_raw(record("120000", "120000", "M", "csharp/link")), "full")
        self.assertEqual(scope_raw(record("100644", "100755", "M", "csharp/run.sh")), "full")
        self.assertEqual(scope_raw(record("160000", "160000", "M", "csharp/sub")), "full")
        self.assertEqual(scope_raw(record("100644", "100644", "T", "csharp/Disunity/A.cs")), "full")

    def test_empty_malformed_and_failed_diffs_are_full(self):
        self.assertEqual(scope_raw(""), "full")
        self.assertEqual(scope_raw("garbage"), "full")

        def fail(*_):
            raise subprocess.CalledProcessError(128, "git")

        self.assertEqual(cc.classify_scope("base", "head", fail)[0], "full")


class GitRepository(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.addCleanup(self.directory.cleanup)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "ci@example.invalid")
        self.git("config", "user.name", "ci")
        self.git("config", "commit.gpgsign", "false")
        self.write("src/lib.rs", "pub fn a() {}\n" * 20)
        self.write("README.md", "# Title\n")
        self.commit("base")
        self.base = self.rev()

    def git(self, *args):
        result = subprocess.run(
            ["git", *args], cwd=self.root, capture_output=True, text=True, check=True
        )
        return result.stdout.strip()

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", message)

    def rev(self):
        return self.git("rev-parse", "HEAD")

    def classify(self):
        previous = os.getcwd()
        os.chdir(self.root)
        try:
            return cc.classify(self.base, self.rev())
        finally:
            os.chdir(previous)

    def test_markdown_only_commit(self):
        self.write("README.md", "# Title\n\nMore.\n")
        self.write("docs/new.md", "# New\n")
        self.commit("docs")
        self.assertTrue(self.classify()[0])

    def test_identical_trees_are_not_docs_only(self):
        self.commit("empty")
        self.assertFalse(self.classify()[0])

    def test_renaming_code_to_markdown_is_code_even_with_rename_detection(self):
        self.git("config", "diff.renames", "true")
        self.git("mv", "src/lib.rs", "docs-lib.md")
        self.commit("rename")
        self.assertFalse(self.classify()[0])

    def test_symlink_named_md_is_code(self):
        (self.root / "link.md").symlink_to("src/lib.rs")
        self.commit("symlink")
        self.assertFalse(self.classify()[0])

    def test_executable_bit_change_on_markdown_is_code(self):
        self.git("update-index", "--chmod=+x", "README.md")
        self.commit("chmod")
        self.assertFalse(self.classify()[0])

    def test_gitlink_named_md_is_code(self):
        self.git("update-index", "--add", "--cacheinfo", f"160000,{SHA_A},vendored.md")
        self.commit("gitlink")
        self.assertFalse(self.classify()[0])

    def test_bad_revision_is_not_docs_only(self):
        previous = os.getcwd()
        os.chdir(self.root)
        try:
            self.assertFalse(cc.classify("no-such-revision", "HEAD")[0])
        finally:
            os.chdir(previous)


class Slugs(unittest.TestCase):
    def slugs(self, text):
        return cc.heading_slugs(text.splitlines())

    def test_github_slug_rules(self):
        self.assertEqual(cc.github_slug("Hello, World!"), "hello-world")
        self.assertEqual(cc.github_slug("A & B"), "a--b")
        self.assertEqual(cc.github_slug("`ci-gpu` plan"), "ci-gpu-plan")
        self.assertEqual(cc.github_slug("See [the guide](x.md)"), "see-the-guide")
        self.assertEqual(cc.github_slug("snake_case Name"), "snake_case-name")

    def test_duplicate_headings_get_numeric_suffixes(self):
        found = self.slugs("# Notes\n## Notes\n### Notes\n")
        self.assertEqual(found, {"notes", "notes-1", "notes-2"})

    def test_headings_inside_fences_are_ignored(self):
        found = self.slugs("# Real\n```\n# Fake\n```\n~~~\n## Also fake\n~~~\n")
        self.assertEqual(found, {"real"})

    def test_setext_headings_and_trailing_hashes(self):
        found = self.slugs("Title\n=====\n\nSub head\n------\n\n## Closed ##\n")
        self.assertEqual(found, {"title", "sub-head", "closed"})

    def test_table_separators_are_not_headings(self):
        self.assertEqual(self.slugs("| a | b |\n|---|---|\n| 1 | 2 |\n"), set())

    def test_html_anchors_are_collected(self):
        self.assertEqual(cc.html_anchors('<a name="Top"></a> <a id="x">'), {"top", "x"})


class Links(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.write("README.md", "# Home\n\n## Setup steps\n")
        self.write("docs/guide.md", "# Guide\n\n## Details\n\n## Details\n")
        self.write("crates/play/src/main.rs", "fn main() {}\n")

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def failures(self, name, text):
        self.write(name, text)
        change = cc.Change("A", "000000", "100644", name)
        return cc.check_docs(self.root, [change])

    def test_valid_links_and_anchors_pass(self):
        text = (
            "# Page\n\n## Local\n"
            "[a](../README.md#setup-steps) [b](guide.md#details-1) [c](#local)\n"
            "[d](/crates/play/src/main.rs) [e](../crates/play) ![i](guide.md)\n"
            "[f](https://example.com/x) [g](mailto:a@b.c) <https://x.y>\n"
            "[h](https://github.com/acoliver/real-disunity/blob/main/docs/guide.md#guide)\n"
            "[l](../crates/play/src/main.rs#L10)\n"
            "[ref][r]\n\n[r]: guide.md#details\n"
        )
        self.assertEqual(self.failures("docs/page.md", text), [])

    def test_missing_files_and_anchors_are_listed_with_line_numbers(self):
        text = "# Page\n[a](missing.md)\n[b](guide.md#nope)\n[c](#absent)\n[d](../../escape.md)\n"
        found = self.failures("docs/page.md", text)
        self.assertEqual(len(found), 4, found)
        self.assertIn("docs/page.md:2: missing target docs/missing.md", found[0])
        self.assertIn("#nope", found[1])
        self.assertIn("#absent", found[2])
        self.assertIn("leaves the repository", found[3])

    def test_repository_urls_map_to_local_paths(self):
        base = "https://github.com/acoliver/real-disunity/blob/main/"
        text = f"[a]({base}docs/gone.md)\n[b]({base}docs/guide.md#nope)\n[c]({base}README.md)\n"
        found = self.failures("page.md", text)
        self.assertEqual(len(found), 2, found)

    def test_code_spans_fences_and_footnotes_are_not_links(self):
        text = "`[a](missing.md)`\n```\n[b](missing.md)\n```\n[^1]: a note\n"
        self.assertEqual(self.failures("page.md", text), [])

    def test_only_added_and_modified_markdown_is_checked(self):
        self.write("broken.md", "[x](missing.md)\n")
        changes = [
            cc.Change("D", "100644", "000000", "broken.md"),
            cc.Change("M", "100644", "100644", "src/lib.rs"),
        ]
        self.assertEqual(cc.check_docs(self.root, changes), [])

    def test_percent_encoded_paths_are_decoded(self):
        self.write("docs/with space.md", "# S\n")
        self.assertEqual(self.failures("page.md", "[a](docs/with%20space.md)\n"), [])


class MarkdownInputs(unittest.TestCase):
    def setUp(self):
        self.enterContext(contextlib.redirect_stdout(io.StringIO()))
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        self.target = self.root / "target/validation"
        (self.target / "debug/deps").mkdir(parents=True)
        self.build = self.target / "debug/build/ffi-0123456789abcdef"
        self.build.mkdir(parents=True)
        (self.root / "csharp/App").mkdir(parents=True)
        (self.root / "crates/ffi").mkdir(parents=True)
        (self.root / "crates/ffi/Cargo.toml").write_text('[package]\nname = "ffi"\n')

    def depinfo(self, text):
        (self.target / "debug/deps/crate.d").write_text(text)

    def run_check(self):
        args = cc.build_parser().parse_args(
            ["markdown-inputs", "--target", str(self.target), "--root", str(self.root)]
        )
        return cc.command_markdown_inputs(args)

    def test_clean_build_passes(self):
        self.depinfo("crate: crates/a/src/lib.rs crates/a/src/x.rs\n\ncrates/a/src/lib.rs:\n")
        (self.build / "output").write_text(
            "cargo:rerun-if-changed=src/abi.rs\ncargo::rerun-if-changed=build.rs\n"
        )
        (self.root / "csharp/App/App.csproj").write_text("<Project><Compile Include='a.cs'/></Project>")
        self.assertEqual(self.run_check(), 0)

    def test_repository_markdown_in_dep_info_fails(self):
        self.depinfo("crate: crates/a/src/lib.rs \\\n crates/a/README.md\n\ncrates/a/README.md:\n")
        self.assertEqual(self.run_check(), 1)

    def test_absolute_repository_markdown_in_dep_info_fails(self):
        self.depinfo(f"crate: {self.root}/docs/guide.md\n")
        self.assertEqual(self.run_check(), 1)

    def test_markdown_from_registry_crates_outside_the_repository_is_ignored(self):
        self.depinfo("crate: /home/u/.cargo/registry/src/x/dep/README.md\n")
        self.assertEqual(self.run_check(), 0)

    def test_escaped_spaces_are_unescaped(self):
        self.assertEqual(cc.dep_info_paths("a: b\\ c.md d\n"), ["a", "b c.md", "d"])
        self.depinfo("crate: docs/my\\ notes.md\n")
        self.assertEqual(self.run_check(), 1)

    def test_build_script_watching_markdown_fails(self):
        self.depinfo("crate: crates/a/src/lib.rs\n")
        (self.build / "output").write_text("cargo:rerun-if-changed=README.md\n")
        self.assertEqual(self.run_check(), 1)

    def test_registry_build_scripts_may_watch_their_own_markdown(self):
        self.depinfo("crate: crates/a/src/lib.rs\n")
        other = self.target / "debug/build/blake3-fedcba9876543210"
        other.mkdir()
        (other / "output").write_text("cargo:rerun-if-changed=c/README.md\n")
        self.assertEqual(self.run_check(), 0)
        (other / "output").write_text(f"cargo:rerun-if-changed={self.root}/docs/guide.md\n")
        self.assertEqual(self.run_check(), 1)

    def test_build_package_names_strip_the_hash(self):
        self.assertEqual(cc.build_package(Path("worldrunner-x-0123456789abcdef")), "worldrunner-x")
        self.assertEqual(cc.build_package(Path("plain")), "plain")

    def test_managed_project_files_mentioning_markdown_fail(self):
        self.depinfo("crate: crates/a/src/lib.rs\n")
        for name in ("A.csproj", "B.props", "C.targets"):
            path = self.root / "csharp/App" / name
            path.write_text('<Project><None Include="../README.md"/></Project>')
            self.assertEqual(self.run_check(), 1, name)
            path.unlink()

    def test_generated_managed_output_is_not_scanned(self):
        self.depinfo("crate: crates/a/src/lib.rs\n")
        generated = self.root / "csharp/App/obj"
        generated.mkdir()
        (generated / "project.nuget.g.props").write_text("README.md")
        self.assertEqual(self.run_check(), 0)

    def test_missing_or_empty_build_output_fails_closed(self):
        self.assertEqual(self.run_check(), 1)
        args = cc.build_parser().parse_args(
            ["markdown-inputs", "--target", str(self.root / "absent"), "--root", str(self.root)]
        )
        self.assertEqual(cc.command_markdown_inputs(args), 1)


class Outputs(unittest.TestCase):
    def setUp(self):
        self.enterContext(contextlib.redirect_stdout(io.StringIO()))

    def test_classify_writes_github_output_and_exits_zero_on_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "out"
            summary = Path(directory) / "summary"
            env = {"GITHUB_OUTPUT": str(output), "GITHUB_STEP_SUMMARY": str(summary)}
            previous = {key: os.environ.get(key) for key in env}
            os.environ.update(env)
            try:
                args = cc.build_parser().parse_args(
                    ["classify", "--base", "no-such-rev", "--head", "no-such-rev"]
                )
                self.assertEqual(cc.command_classify(args), 0)
            finally:
                for key, value in previous.items():
                    if value is None:
                        os.environ.pop(key, None)
                    else:
                        os.environ[key] = value
            self.assertEqual(output.read_text(), "docs_only=false\nscope=full\n")
            self.assertIn("scope: full", summary.read_text())


if __name__ == "__main__":
    unittest.main()
