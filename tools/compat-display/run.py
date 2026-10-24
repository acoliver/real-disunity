#!/usr/bin/env python3
"""Run one command with a private, worktree-backed X11 display namespace."""

import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / ".work/display-resumed"


def stop_group(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        try:
            os.killpg(process.pid, 0)
        except ProcessLookupError:
            return
        time.sleep(0.05)
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def host_paths(number):
    return (Path(f"/tmp/.X{number}-lock"), Path(f"/tmp/.X11-unix/X{number}"))


def display_number():
    for number in range(98, 120):
        if not any(path.exists() for path in host_paths(number)):
            return number
    raise RuntimeError("no unused X display number from :98 through :119")


def server_command(display, witness):
    code = ("import json,os,sys; s=os.stat('/tmp'); "
            "json.dump({'namespace':os.readlink('/proc/self/ns/mnt'),"
            "'tmp':[s.st_dev,s.st_ino]},open(sys.argv[1],'w')); "
            "os.execvp('Xvfb',['Xvfb',sys.argv[2],'-screen','0','1280x800x24','-nolisten','tcp'])")
    return [sys.executable, "-c", code, str(witness), display]


def wait_display(server, display):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if server.poll() is not None:
            raise RuntimeError(f"Xvfb exited early with status {server.returncode}")
        try:
            probe = subprocess.run(["xdpyinfo", "-display", display],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=1)
            if probe.returncode == 0:
                return
        except (subprocess.TimeoutExpired, FileNotFoundError):
            pass
        time.sleep(0.1)
    raise RuntimeError("Xvfb did not become ready within 10 seconds")


def child_environment(session, display):
    runtime = session / "runtime"
    runtime.mkdir(mode=0o700)
    os.chmod(runtime, 0o700)
    env = os.environ.copy()
    env.update(DISPLAY=display, TMPDIR="/tmp", TMP="/tmp", TEMP="/tmp",
               COMPAT_DISPLAY_CONTAINMENT=str(session / "containment.json"),
               HOME=str(session / "home"), XDG_RUNTIME_DIR=str(runtime),
               DOTNET_CLI_HOME=str(ROOT / ".work/dotnet-home"),
               NUGET_PACKAGES=str(ROOT / ".work/nuget"),
               DOTNET_SKIP_FIRST_TIME_EXPERIENCE="1", DOTNET_CLI_TELEMETRY_OPTOUT="1",
               VK_DRIVER_FILES="/usr/share/vulkan/icd.d/lvp_icd.json")
    return env


def containment(session, number, host_ns, host_tmp, command, witness):
    display = f":{number}"
    local_tmp = Path("/tmp").stat()
    lock, socket = host_paths(number)
    server = json.loads(witness.read_text())
    namespace = os.readlink("/proc/self/ns/mnt")
    if namespace == host_ns or namespace != server["namespace"]:
        raise RuntimeError("mount namespace isolation was not proven for runner and Xvfb")
    if (local_tmp.st_dev, local_tmp.st_ino) == host_tmp:
        raise RuntimeError("private /tmp resolves to the host /tmp")
    if server["tmp"] != [local_tmp.st_dev, local_tmp.st_ino]:
        raise RuntimeError("Xvfb /tmp does not match the runner's private /tmp")
    return {"display": display, "mount_namespace": namespace, "host_mount_namespace": host_ns,
            "xvfb_mount_namespace": server["namespace"],
            "private_tmp_device_inode": [local_tmp.st_dev, local_tmp.st_ino],
            "xvfb_tmp_device_inode": server["tmp"], "host_tmp_device_inode": list(host_tmp),
            "private_lock_device_inode": [lock.stat().st_dev, lock.stat().st_ino],
            "private_socket_device_inode": [socket.lstat().st_dev, socket.lstat().st_ino],
            "runtime_dir": str(session / "runtime"),
            "runtime_mode": oct((session / "runtime").stat().st_mode & 0o777),
            "command": command}


def run_inside(session, number, host_ns, host_dev, host_ino, command):
    def interrupt(signum, _frame):
        raise KeyboardInterrupt(f"received signal {signum}")

    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, interrupt)
    display = f":{number}"
    logs = [ (session / name).open("wb") for name in
             ("xvfb.stdout.log", "xvfb.stderr.log", "command.stdout.log", "command.stderr.log") ]
    out, err, child_out, child_err = logs
    server = subprocess.Popen(server_command(display, session / "xvfb-context.json"),
                              stdin=subprocess.DEVNULL, stdout=out, stderr=err, start_new_session=True)
    (session / "xvfb.pid").write_text(f"{server.pid}\n")
    status = 125
    try:
        wait_display(server, display)
        env = child_environment(session, display)
        evidence = containment(session, number, host_ns, (host_dev, host_ino), command,
                              session / "xvfb-context.json")
        (session / "containment.json").write_text(json.dumps(evidence, indent=2) + "\n")
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=child_out, stderr=child_err,
                                   start_new_session=True)
        (session / "command.pid").write_text(f"{process.pid}\n")
        try:
            status = process.wait()
        except BaseException:
            stop_group(process)
            process.wait()
            raise
        finally:
            stop_group(process)
    except BaseException as error:
        (session / "runner-error.txt").write_text(f"{type(error).__name__}: {error}\n")
        status = 125
    finally:
        if server.poll() is None:
            server.terminate()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()
        (session / "result.txt").write_text(f"exit_status={status}\nxvfb_status={server.returncode}\n")
        (session / "command.pid").unlink(missing_ok=True)
        (session / "xvfb.pid").unlink(missing_ok=True)
        for stream in logs:
            stream.close()
    return status


def make_session():
    BASE.mkdir(parents=True, exist_ok=True)
    path = BASE / "runs" / f"{time.strftime('%Y%m%d-%H%M%S')}-{uuid.uuid4().hex[:10]}"
    (path / "tmp/.X11-unix").mkdir(parents=True)
    (path / "home").mkdir()
    return path


def bwrap_command(session, number, command):
    tmp = Path("/tmp").stat()
    return ["bwrap", "--die-with-parent", "--new-session", "--unshare-user", "--unshare-net",
            "--ro-bind", "/", "/", "--bind", str(ROOT), str(ROOT),
            "--bind", str(session / "tmp"), "/tmp", "--proc", "/proc", "--dev", "/dev",
            "--", sys.executable, str(Path(__file__).resolve()), "--inside", str(session),
            str(number), os.readlink("/proc/self/ns/mnt"), str(tmp.st_dev), str(tmp.st_ino), *command]


def run_host(command):
    if not all(shutil.which(program) for program in ("bwrap", "Xvfb", "xdpyinfo")):
        print("bwrap, Xvfb and xdpyinfo are required", file=sys.stderr)
        return 2
    number = display_number()
    session = make_session()
    print(f"session={session}", flush=True)
    paths = host_paths(number)
    before = {str(path): path.exists() for path in paths}
    if any(before.values()):
        raise RuntimeError("host display paths appeared before runner startup")
    with (session / "launcher.stdout.log").open("wb") as out, (session / "launcher.stderr.log").open("wb") as err:
        proc = subprocess.Popen(bwrap_command(session, number, command), cwd=ROOT, stdout=out,
                                 stderr=err, start_new_session=True)
        return run_wait(proc, session, number, paths, before)


def run_wait(proc, session, number, paths, before):
    interrupted = []
    def stop(signum, _frame):
        interrupted.append(signum)
        command_pid = session / "command.pid"
        try:
            os.killpg(int(command_pid.read_text()), signum)
        except (FileNotFoundError, ProcessLookupError, ValueError):
            os.killpg(proc.pid, signum)
    previous = {sig: signal.signal(sig, stop) for sig in (signal.SIGINT, signal.SIGTERM)}
    try:
        status = proc.wait()
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)
    after = {str(path): path.exists() for path in paths}
    (session / "host-display-state.json").write_text(json.dumps({
        "display": f":{number}", "host_paths_before": before,
        "host_paths_after": after, "interrupted_by": interrupted}, indent=2) + "\n")
    if before != after or any(after.values()):
        raise RuntimeError("host X11 display paths changed during contained run")
    if (session / "containment.json").exists():
        details = json.loads((session / "containment.json").read_text())
        print(f"namespace={details['mount_namespace']} display={details['display']}")
    return status


def main():
    if "--inside" in sys.argv:
        marker = sys.argv.index("--inside")
        values = sys.argv[marker + 1:]
        session, number, host_ns, host_dev, host_ino = values[:5]
        return run_inside(Path(session), int(number), host_ns, int(host_dev), int(host_ino), values[5:])
    if len(sys.argv) < 3 or sys.argv[1] != "--":
        print(f"usage: {sys.argv[0]} -- COMMAND [ARG ...]", file=sys.stderr)
        return 2
    return run_host(sys.argv[2:])


if __name__ == "__main__":
    raise SystemExit(main())
