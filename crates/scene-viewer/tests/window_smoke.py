#!/usr/bin/env python3
"""Opt-in pixel-behavior smoke for the facade example on exclusive Xvfb :99.

Requires Pillow, xdotool, and the built facade example. Never uses the inherited
user desktop. Output must be a new or empty directory (not a file or symlink).
Commands have a 5-second / 64-KiB combined-output budget; viewer logs are capped
at 1 MiB each. Child process groups are cleaned up on every exit path.
VK_DRIVER_FILES and LD_LIBRARY_PATH are caller-owned test settings.
"""

import argparse
from contextlib import contextmanager, nullcontext
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import threading
import time

from PIL import Image, ImageChops, ImageGrab, ImageStat


def signal_group(process, sig):
    try:
        os.killpg(process.pid, sig)
    except ProcessLookupError:
        pass


@contextmanager
def managed_process(command, environment, *, log_path=None, output_limit=1024 * 1024):
    """Bound combined stdout/stderr and own the complete child process group."""
    with log_path.open("xb") if log_path else nullcontext(None) as log:
        process = subprocess.Popen(
            command, env=environment, stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
        )
        stdout, stderr = bytearray(), bytearray()
        failures = []
        stopped = threading.Event()

        def collect():
            remaining = output_limit
            try:
                with selectors.DefaultSelector() as selector:
                    selector.register(process.stdout, selectors.EVENT_READ, stdout)
                    selector.register(process.stderr, selectors.EVENT_READ, stderr)
                    while selector.get_map() and not stopped.is_set():
                        for key, _ in selector.select(timeout=0.1):
                            chunk = os.read(key.fd, 4096)
                            if not chunk:
                                selector.unregister(key.fileobj)
                                continue
                            bounded = chunk[:remaining]
                            if log:
                                log.write(bounded)
                            else:
                                key.data.extend(bounded)
                            remaining -= len(bounded)
                            if len(chunk) > len(bounded):
                                raise RuntimeError(f"Process exceeded output budget ({output_limit} bytes): {command}")
            except Exception as error:
                failures.append(error)
                signal_group(process, signal.SIGKILL)

        reader = threading.Thread(target=collect, daemon=True)
        reader.start()
        try:
            yield process, stdout, stderr
        finally:
            try:
                signal_group(process, signal.SIGTERM)
                try:
                    process.wait(timeout=1)
                except subprocess.TimeoutExpired:
                    pass
                finally:
                    # Also kill descendants when the group leader has already exited.
                    signal_group(process, signal.SIGKILL)
                process.wait(timeout=5)
                reader.join(timeout=2)
                if reader.is_alive():
                    raise RuntimeError(f"Process output pipes did not close: {command}")
            finally:
                stopped.set()
                reader.join(timeout=1)
                process.stdout.close()
                process.stderr.close()
            if failures:
                raise failures[0]


def run_command(command, environment, *, check=True, timeout=5, output_limit=64 * 1024):
    with managed_process(command, environment, output_limit=output_limit) as (process, stdout, stderr):
        code = process.wait(timeout=timeout)
    result = subprocess.CompletedProcess(command, code, stdout.decode(errors="replace"),
                                         stderr.decode(errors="replace"))
    if check:
        result.check_returncode()
    return result


def prepare_output(output):
    if output.is_symlink() or (output.exists() and not output.is_dir()):
        raise ValueError("output must be a new or empty directory, not a file or symlink")
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError("output directory must be empty; existing evidence is never overwritten")
    return output.resolve()


def wait_for(predicate, label, output):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.05)
    screenshot().save(output / "timeout.png")
    raise AssertionError(f"Timed out waiting for {label}")


def screenshot():
    return ImageGrab.grab(xdisplay=":99")


def different(left, right):
    return ImageChops.difference(left, right).getbbox()


def run_mode(binary, xdotool, output, mode, environment):
    def xdo(*arguments, check=True):
        result = run_command(
            [str(xdotool), *map(str, arguments)], environment, check=check,
        )
        return result.stdout.strip()

    with managed_process(
        [str(binary), *(["--shadows"] if mode == "shadowed" else [])], environment,
        log_path=output / f"{mode}.log",
    ) as (process, _, _):
        window = wait_for(
            lambda: xdo("search", "--pid", process.pid, "--name",
                        "^Generic scene viewer$", check=False),
            "window creation", output,
        )
        xdo("windowfocus", window)

        def nonblack_frame():
            image = screenshot()
            return image if image.getbbox() else None

        initial = wait_for(nonblack_frame, "first rendered frame", output)
        initial.save(output / f"{mode}-initial.png")
        colors = initial.getcolors(2_000_000)
        assert colors is not None
        assert any(count > 1000 and red > 100 and green < 10 and blue < 10
                   for count, (red, green, blue) in colors), colors

        def changed_frame():
            image = screenshot()
            return image if different(initial, image) else None

        xdo("mousemove", "--window", window, 400, 300)
        xdo("mousedown", 1)
        xdo("mousemove", "--window", window, 500, 360)
        xdo("mouseup", 1)
        dragged = wait_for(changed_frame, "drag changing pixels", output)
        dragged.save(output / f"{mode}-dragged.png")

        xdo("windowfocus", window)
        xdo("key", "--window", window, "r")
        wait_for(lambda: not different(initial, screenshot()),
                 "R exact reset", output)
        xdo("click", 4)
        zoomed = wait_for(changed_frame, "wheel changing pixels", output)
        zoomed.save(output / f"{mode}-zoomed.png")

        xdo("windowfocus", window)
        xdo("key", "--window", window, "Home")
        wait_for(lambda: not different(initial, screenshot()),
                 "Home exact reset", output)
        xdo("windowsize", window, 640, 480)
        wait_for(lambda: "WIDTH=640" in xdo("getwindowgeometry", "--shell", window),
                 "resize event", output)
        resized = wait_for(changed_frame, "resized frame", output)
        resized.save(output / f"{mode}-resized.png")

        xdo("windowunmap", window)
        wait_for(lambda: not screenshot().getbbox(), "window unmapping", output)
        xdo("windowmap", window)
        wait_for(nonblack_frame, "restored rendering", output)
        xdo("windowfocus", window)
        xdo("key", "--window", window, "Escape")
        code = process.wait(timeout=15)
        assert code == 0, code
        return {
            "mode": mode,
            "exit": code,
            "controls": ["drag changes pixels", "wheel changes pixels",
                         "R exact reset", "Home exact reset", "resize 640x480",
                         "unmap/map restoration", "Escape clean exit"],
            "initial_nonblack_pixels": sum(count for count, color in colors
                                           if color != (0, 0, 0)),
        }


def run_resize_mode(binary, xdotool, output, mode, environment):
    # This is a test-process DPI override, not viewer/engine environment policy.
    environment = dict(environment, WINIT_X11_SCALE_FACTOR="2")

    def xdo(*arguments, check=True):
        return run_command(
            [str(xdotool), *map(str, arguments)], environment, check=check,
        ).stdout.strip()

    def geometry(window):
        fields = dict(line.split("=", 1) for line in
                      xdo("getwindowgeometry", "--shell", window).splitlines())
        return tuple(int(fields[key]) for key in ("X", "Y", "WIDTH", "HEIGHT"))

    def capture(window):
        x, y, width, height = geometry(window)
        return ImageGrab.grab(bbox=(x, y, x + width, y + height), xdisplay=":99")

    def mask(image):
        return image.getchannel("R").point(lambda red: 255 if red > 100 else 0)

    def normalized(image):
        return mask(image).resize((240, 240), Image.Resampling.NEAREST)

    records = []
    with managed_process(
        [str(binary), *(["--shadows"] if mode == "shadowed" else [])], environment,
        log_path=output / f"{mode}-oversized.log",
    ) as (process, _, _):
        window = wait_for(
            lambda: xdo("search", "--pid", process.pid, "--name",
                        "^Generic scene viewer$", check=False),
            "HiDPI window", output,
        )
        xdo("windowmove", window, 0, 0)
        wait_for(lambda: geometry(window)[2:] == (1600, 1200),
                 "800x600 logical at 2x physical", output)
        for small, large in [
            ((800, 450), (5120, 2880)),
            ((450, 800), (2880, 5120)),
            ((1000, 400), (5001, 2003)),
        ]:
            xdo("windowsize", window, *small)
            wait_for(lambda: geometry(window)[2:] == small, "baseline resize", output)

            # Wait for projection to catch up, not just X11's geometry.
            # The expected quad is centred horizontally in the example.
            def centred_frame():
                image = capture(window)
                bounds = mask(image).getbbox()
                if bounds and abs((bounds[0] + bounds[2]) / 2 - small[0] / 2) < 2:
                    return image
                return None

            baseline = wait_for(centred_frame, "baseline projection", output)
            expected = normalized(baseline)
            xdo("windowsize", window, *large)
            wait_for(lambda: geometry(window)[2:] == large, "oversized native extent", output)

            def aspect_correct_frame():
                image = capture(window)
                if image.size != large:
                    return None
                difference = ImageStat.Stat(ImageChops.difference(expected, normalized(image))).mean[0] / 255
                return (image, difference) if difference < 0.01 else None

            image, difference = wait_for(aspect_correct_frame, "aspect-correct oversized pixels", output)
            image.save(output / f"{mode}-{large[0]}x{large[1]}.png")
            records.append({"physical": large, "baseline": small,
                            "normalized_mask_error": difference,
                            "red_bounds": mask(image).getbbox()})
        xdo("windowsize", window, 640, 480)
        wait_for(lambda: geometry(window)[2:] == (640, 480) and mask(capture(window)).getbbox(),
                 "regular resize after oversize", output)
        xdo("windowfocus", window)
        xdo("key", "--window", window, "Escape")
        assert process.wait(timeout=15) == 0
        return {"mode": mode, "dpi_scale": 2, "initial_physical": [1600, 1200],
                "resizes": records, "exit": 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--xdotool", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--oversized", action="store_true",
                        help="also test 2x DPI and >4096 extents; Xvfb screen must be at least 5120x5120")
    args = parser.parse_args()
    try:
        output = prepare_output(args.output)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    environment = dict(os.environ, DISPLAY=":99", WAYLAND_DISPLAY="")
    environment.pop("WAYLAND_SOCKET", None)
    results = [run_mode(args.binary.resolve(), args.xdotool.resolve(), output,
                        mode, environment) for mode in ["plain", "shadowed"]]
    if args.oversized:
        results.extend(run_resize_mode(args.binary.resolve(), args.xdotool.resolve(), output,
                                       mode, environment) for mode in ["plain", "shadowed"])
    encoded = json.dumps(results, indent=2) + "\n"
    with (output / "results.json").open("x") as result_file:
        result_file.write(encoded)
    print(encoded, end="")


if __name__ == "__main__":
    main()
