"""Validate executable locations without running providers or assigning acceptance."""
import ast
import hashlib
import json
import re

from model import Invalid, load, require


def digest(data):
    return hashlib.sha256(data).hexdigest()


def local_path(root, name):
    path = root / name
    require(not path.is_absolute() or not str(name).startswith("/"), "absolute path rejected")
    require(path.resolve().is_relative_to(root.resolve()), f"path escapes root: {name}")
    require(path.is_file(), f"missing path: {name}")
    return path


def pointer(value, location):
    require(not location or location.startswith("/"), "invalid JSON pointer")
    try:
        for part in location.split("/")[1:]:
            key = part.replace("~1", "/").replace("~0", "~")
            value = value[int(key)] if isinstance(value, list) else value[key]
        return value
    except (KeyError, IndexError, ValueError, TypeError) as error:
        raise Invalid(f"absent record pointer {location}") from error


def mask_code(text):
    # Preserve offsets so declarations in strings and comments cannot supply symbols.
    pattern = r'/\*.*?\*/|//[^\n]*|r(#+)?".*?"\1|"(?:\\.|[^"\\])*"|\x27(?:\\.|[^\x27\\])\x27'
    return re.sub(pattern, lambda m: " " * len(m[0]), text, flags=re.S)


def block(text, start):
    masked = mask_code(text)
    begin = masked.find("{", start)
    require(begin >= 0, "symbol has no executable block")
    depth = 1
    for end in range(begin + 1, len(masked)):
        depth += (masked[end] == "{") - (masked[end] == "}")
        if depth == 0:
            return text[begin:end + 1]
    raise Invalid("unterminated symbol block")


def rust_body(text, symbol):
    masked = mask_code(text)
    matches = list(re.finditer(r"\bfn\s+" + re.escape(symbol) + r"\s*\(", masked))
    require(len(matches) == 1, f"absent or ambiguous test symbol: {symbol}")
    match = matches[0]
    prefix = masked[max(0, match.start() - 300):match.start()]
    require(re.search(r"#\[test\]\s*(?:#\[[^\]]+\]\s*)*$", prefix),
            f"symbol is not a registered Rust test: {symbol}")
    return block(text, match.start())


def python_body(text, symbol):
    tree = ast.parse(text)
    names = symbol.split(".")
    for name in names:
        matches = [node for node in tree.body if isinstance(
            node, (ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == name]
        require(len(matches) == 1, f"absent python symbol: {symbol}")
        tree = matches[0]
    require(isinstance(tree, (ast.FunctionDef, ast.AsyncFunctionDef)), "symbol is not executable")
    return "\n".join(text.splitlines()[tree.lineno - 1:tree.end_lineno])


def csharp_body(text, symbol):
    owner, method = symbol.split(".")
    masked = mask_code(text)
    match = re.search(r"\bclass\s+" + re.escape(owner) + r"\b", masked)
    require(match, f"absent class symbol: {owner}")
    body = block(text, match.start())
    found = re.search(r"\b(?:public|internal)\s+static\s+\w+\s+" +
                      re.escape(method) + r"\s*\(", mask_code(body))
    require(found, f"absent C# test symbol: {symbol}")
    return block(body, found.start())


def validate_code(ref, path, root):
    text = path.read_text()
    body = {"rust-test": rust_body, "python-check": python_body,
            "csharp-test": csharp_body}[ref["kind"]](text, ref["symbol"])
    require(digest(path.read_bytes()) == ref["source_sha256"], f"stale source digest: {path}")
    require(ref["anchor"] and ref["anchor"] in body, f"assertion anchor absent: {ref['symbol']}")
    require(isinstance(ref["command"], list) and ref["command"], "executable command missing")
    if ref["kind"] == "csharp-test":
        registration = local_path(root, ref["registration"]["path"]).read_text()
        require(ref["registration"]["anchor"] in registration, "test registration absent")
    if "execution" in ref:
        validate_execution(ref, root)
    if "metadata_probe" in ref:
        import package_checks
        require(getattr(package_checks, ref["symbol"])(root) == ref["metadata_probe"],
                "original package metadata evidence mismatch")


def validate_execution(ref, root):
    execution = ref["execution"]
    path = local_path(root, execution["path"])
    require(digest(path.read_bytes()) == execution["sha256"], "recorded execution log digest mismatch")
    require(execution["source_binding"] == "unverified-historical",
            "historical execution cannot establish current source acceptance")
    symbol = ref["symbol"]
    if ref["kind"] == "csharp-test":
        symbol = "original_source_facades_and_native_lifecycle_suites_execute_in_managed_ci"
    marker = execution["marker"]
    require(re.fullmatch(r"test (?:\w+::)*" + re.escape(symbol) + r" \.\.\. ok", marker),
            "execution marker does not identify selected test symbol")
    require(marker in path.read_text().splitlines(), "recorded test execution marker absent")


def validate_assertion(ref, path, root):
    fixture = load(path)
    index = ref["index"]
    require(type(index) is int and 0 <= index < len(fixture["assertions"]),
            f"fixture assertion index absent: {index}")
    require(fixture["assertions"][index] == ref["expected"], "fixture assertion mapping changed")
    require(ref["profile"] in fixture["profiles"], "fixture profile mismatch")
    require(ref["fidelity"] == fixture["evidence_label"], "fixture fidelity mismatch")
    if "record" in ref:
        record = load(local_path(root, ref["record"]))
        require(record["profile"] == ref["profile"], "recorded profile mismatch")
        require(record["evidence_label"] == ref["fidelity"], "recorded fidelity mismatch")
        require(index < len(record["assertions"]), "recorded assertion index absent")
        result = record["assertions"][index]
        require(result["assertion"] == ref["expected"], "recorded assertion mapping mismatch")
        require(result["passed"] is True, "recorded assertion did not pass")


def validate_reference(ref, root):
    path = local_path(root, ref["path"])
    kind = ref["kind"]
    if kind in {"rust-test", "python-check", "csharp-test"}:
        validate_code(ref, path, root)
    elif kind == "fixture-assertion":
        validate_assertion(ref, path, root)
    elif kind == "record":
        require(pointer(load(path), ref["pointer"]) == ref["expected"],
                f"record identity/value mismatch: {ref['path']}{ref['pointer']}")
    elif kind == "run-proof":
        from records import validate_run_reference
        validate_run_reference(ref, root)
    elif kind == "file-digest":
        require(digest(path.read_bytes()) == ref["sha256"], f"record digest mismatch: {path}")
    else:
        raise Invalid(f"unknown evidence kind: {kind}")
