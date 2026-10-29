#!/usr/bin/env python3
"""Decides the required `PR gate` check from the results of the jobs it needs.

The classify job reports a scope. Each scope names the build jobs that must succeed; every
other build job must be skipped. A failed, cancelled or unexpectedly skipped job fails the
gate, so a skipped job can never stand in for a pass. Reads `toJSON(needs)` from NEEDS.
Standard library only.
"""

import json
import os
import sys

ALWAYS = ("classify", "docs")
REQUIRED = {
    "docs": (),
    "csharp": ("cpu", "gpu-csharp"),
    "full": ("cpu", "gpu-static", "gpu-tests"),
}
BUILDS = ("cpu", "gpu-static", "gpu-tests", "gpu-csharp")


def problems(needs: dict) -> list[str]:
    def result(name: str) -> str | None:
        return needs.get(name, {}).get("result")

    found = [
        f"{name} finished as {result(name)!r}, expected 'success'"
        for name in ALWAYS
        if result(name) != "success"
    ]
    scope = needs.get("classify", {}).get("outputs", {}).get("scope")
    if scope not in REQUIRED:
        found.append(f"classify reported scope={scope!r}, expected one of {sorted(REQUIRED)}")
        return found
    for name in BUILDS:
        expected = "success" if name in REQUIRED[scope] else "skipped"
        if result(name) != expected:
            found.append(f"{name} finished as {result(name)!r}, expected {expected!r} for scope={scope}")
    return found


def main() -> int:
    needs = json.loads(os.environ["NEEDS"])
    found = problems(needs)
    if found:
        print("PR gate failed:")
        for problem in found:
            print(f"  - {problem}")
        return 1
    print(f"PR gate passed (scope={needs['classify']['outputs']['scope']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
