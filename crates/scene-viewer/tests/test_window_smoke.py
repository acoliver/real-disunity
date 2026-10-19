"""Display-free safety checks for the opt-in window smoke harness."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import window_smoke as smoke


class ProcessTests(unittest.TestCase):
    def command(self, source):
        return [sys.executable, "-c", source]

    def test_success_and_nonzero_status_preserve_separate_output(self):
        command = self.command(
            "import sys; print('window'); print('diagnostic', file=sys.stderr); sys.exit(3)"
        )
        result = smoke.run_command(command, os.environ, check=False)
        self.assertEqual(result.returncode, 3)
        self.assertEqual(result.stdout, "window\n")
        self.assertEqual(result.stderr, "diagnostic\n")
        with self.assertRaises(subprocess.CalledProcessError) as raised:
            smoke.run_command(command, os.environ)
        self.assertEqual(raised.exception.stderr, "diagnostic\n")

    def test_timeout_kills_process_group_even_when_term_is_ignored(self):
        with tempfile.TemporaryDirectory() as temporary:
            pid_file = Path(temporary) / "pids.json"
            command = self.command(
                "import json, os, signal, subprocess, sys, time; "
                "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
                "child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)']); "
                f"open({str(pid_file)!r}, 'w').write(json.dumps([os.getpid(), child.pid])); "
                "time.sleep(60)"
            )
            with self.assertRaises(subprocess.TimeoutExpired):
                smoke.run_command(command, os.environ, timeout=1)
            self.assertTrue(pid_file.exists(), "timeout fixture did not start")
            for pid in json.loads(pid_file.read_text()):
                status = Path(f"/proc/{pid}/status")
                if status.exists():
                    self.assertIn("\nState:\tZ", status.read_text(),
                                  f"process {pid} survived timeout cleanup")

    def test_command_output_budget_counts_both_streams(self):
        command = self.command(
            "import os; os.write(1, b'x' * 600); os.write(2, b'y' * 600)"
        )
        with self.assertRaisesRegex(RuntimeError, "output budget"):
            smoke.run_command(command, os.environ, output_limit=1024)

    def test_viewer_log_is_capped_and_process_stopped(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "viewer.log"
            command = self.command("import os\nwhile True: os.write(2, b'x' * 4096)")
            with self.assertRaisesRegex(RuntimeError, "output budget"):
                with smoke.managed_process(command, os.environ, log_path=log,
                                           output_limit=1024) as (process, _, _):
                    process.wait(timeout=5)
            self.assertLessEqual(log.stat().st_size, 1024)
            self.assertIsNotNone(process.poll())


class OutputTests(unittest.TestCase):
    def test_new_and_empty_directories_are_accepted(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "new" / "evidence"
            self.assertEqual(smoke.prepare_output(output), output.resolve())
            self.assertTrue(output.is_dir())
            self.assertEqual(smoke.prepare_output(output), output.resolve())

    def test_nonempty_directory_preserves_all_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            contents = {"results.json": b"old results\n", "plain.log": b"old log\n",
                        ".hidden": b"hidden evidence\n"}
            for name, content in contents.items():
                (output / name).write_bytes(content)
            with self.assertRaisesRegex(ValueError, "empty"):
                smoke.prepare_output(output)
            self.assertEqual({path.name: path.read_bytes() for path in output.iterdir()},
                             contents)

    def test_regular_file_and_symlink_are_rejected_without_modification(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            regular = root / "file"
            regular.write_text("preserve")
            directory = root / "directory"
            directory.mkdir()
            link = root / "link"
            link.symlink_to(directory, target_is_directory=True)
            dangling = root / "dangling"
            dangling.symlink_to(root / "missing", target_is_directory=True)
            for output in (regular, link, dangling):
                with self.subTest(output=output):
                    with self.assertRaises(ValueError):
                        smoke.prepare_output(output)
            self.assertEqual(regular.read_text(), "preserve")
            self.assertEqual(list(directory.iterdir()), [])
            self.assertFalse((root / "missing").exists())

    def test_cli_rejects_nonempty_output_before_launching(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            evidence = output / "results.json"
            evidence.write_bytes(b"preserve previous results\n")
            result = subprocess.run(
                [sys.executable, str(Path(smoke.__file__)), "--binary", "/missing/viewer",
                 "--xdotool", "/missing/xdotool", "--output", str(output)],
                capture_output=True, text=True, timeout=5,
            )
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("empty", result.stderr)
            self.assertNotIn("Traceback", result.stderr)
            self.assertEqual(evidence.read_bytes(), b"preserve previous results\n")
            self.assertEqual(list(output.iterdir()), [evidence])


if __name__ == "__main__":
    unittest.main()
