import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
RUNNER = ROOT / "tools/compat-display/run.py"
BASE = ROOT / ".work/display-resumed"


class ContainedDisplayTests(unittest.TestCase):
    def run_command(self, command):
        return subprocess.run([sys.executable, str(RUNNER), "--", *command], cwd=ROOT,
                              text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              timeout=45)

    def session_from(self, result):
        self.assertIn("session=", result.stdout)
        return Path(result.stdout.split("session=", 1)[1].splitlines()[0])

    def test_success_preserves_literal_arguments_and_records_containment(self):
        marker = BASE / f"argv-{os.getpid()}.json"
        result = self.run_command([sys.executable, "-c",
                                   "import json,sys; open(sys.argv[1],'w').write(json.dumps(sys.argv[2:]))",
                                   str(marker), "spaces stay", "$(no-shell)", "semi;colon"])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(marker.read_text()), ["spaces stay", "$(no-shell)", "semi;colon"])
        session = self.session_from(result)
        evidence = json.loads((session / "containment.json").read_text())
        self.assertNotEqual(evidence["mount_namespace"], evidence["host_mount_namespace"])
        self.assertEqual(evidence["private_tmp_device_inode"], evidence["xvfb_tmp_device_inode"])
        self.assertEqual(evidence["mount_namespace"], evidence["xvfb_mount_namespace"])
        self.assertNotEqual(evidence["private_tmp_device_inode"], evidence["host_tmp_device_inode"])
        self.assertNotEqual(evidence["private_lock_device_inode"], evidence["private_socket_device_inode"])
        self.assertEqual(evidence["runtime_mode"], "0o700")
        runtime = Path(evidence["runtime_dir"])
        self.assertTrue(runtime.is_dir())
        self.assertEqual(runtime.stat().st_mode & 0o777, 0o700)
        host_state = json.loads((session / "host-display-state.json").read_text())
        self.assertEqual(host_state["host_paths_before"], host_state["host_paths_after"])
        self.assertFalse((session / "xvfb.pid").exists())
        marker.unlink()

    def test_real_x_geometry_and_coreclr_startup(self):
        dotnet = Path(os.environ["DOTNET_ROOT"]) / "dotnet"
        self.assertTrue(dotnet.is_file(), f"expected configured CoreCLR at {dotnet}")
        probe = self.run_command(["sh", "-c",
                                  "xdpyinfo -display \"$DISPLAY\" | grep -F 'dimensions:    1280x800 pixels' && \"$DOTNET_ROOT/dotnet\" --info"])
        self.assertEqual(probe.returncode, 0, probe.stderr)
        session = self.session_from(probe)
        output = (session / "command.stdout.log").read_text()
        self.assertIn("dimensions:    1280x800 pixels", output)
        self.assertIn("Host:", output)
        self.assertIn("Version:", output)

    def test_child_failure_status_and_output_are_preserved(self):
        result = self.run_command(["sh", "-c", "echo stdout-captured; echo stderr-captured >&2; exit 37"])
        self.assertEqual(result.returncode, 37, result.stderr)
        session = self.session_from(result)
        self.assertIn("exit_status=37", (session / "result.txt").read_text())
        self.assertIn("stdout-captured", (session / "command.stdout.log").read_text())
        self.assertIn("stderr-captured", (session / "command.stderr.log").read_text())
        self.assertFalse((session / "xvfb.pid").exists())
        state = json.loads((session / "host-display-state.json").read_text())
        self.assertEqual(state["host_paths_before"], state["host_paths_after"])

    def test_interrupt_cleans_the_command_group_and_private_display(self):
        wrapper = subprocess.Popen([sys.executable, str(RUNNER), "--", sys.executable,
                                    "-c", "import time; time.sleep(60)"], cwd=ROOT,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        session = Path(wrapper.stdout.readline().strip().removeprefix("session="))
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline and not (session / "containment.json").exists():
            if wrapper.poll() is not None:
                break
            time.sleep(0.05)
        self.assertTrue((session / "containment.json").exists(), "contained command never became ready")
        evidence = json.loads((session / "containment.json").read_text())
        wrapper.send_signal(signal.SIGTERM)
        wrapper.communicate(timeout=15)
        self.assertIsNotNone(wrapper.returncode)
        self.assertFalse((session / "xvfb.pid").exists())
        self.assertFalse((session / "command.pid").exists())
        host_state_path = session / "host-display-state.json"
        self.assertTrue(host_state_path.exists())
        state = json.loads(host_state_path.read_text())
        self.assertEqual(state["host_paths_before"], state["host_paths_after"])
        self.assertIn(signal.SIGTERM, state["interrupted_by"])
        self.assertNotEqual(evidence["private_tmp_device_inode"], evidence["host_tmp_device_inode"])

    def test_usage_rejects_missing_command_before_starting_display(self):
        result = subprocess.run([sys.executable, str(RUNNER)], cwd=ROOT,
                                text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage:", result.stderr)


if __name__ == "__main__":
    unittest.main()
