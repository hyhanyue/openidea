#!/usr/bin/env python3
"""Launch a prepared official server with stdio and a separate per-project cache."""
import fcntl
import hashlib
import json
import os
import pathlib
import signal
import subprocess
import sys


def normalize_completion_filters(message):
    result = message.get("result")
    items = result.get("items", []) if isinstance(result, dict) else result
    if isinstance(items, list):
        for item in items:
            if isinstance(item, dict) and "textEdit" in item and item.get("filterText") == "." + item.get("label", ""):
                item["filterText"] = item["label"]
    return message


def normalize_spring_completion(message):
    result = message.get("result")
    # Spring resolves the complete edit and removes its fallback edit command.
    # Older clients merge optional fields, retaining that command and editing twice.
    if isinstance(result, dict) and isinstance(result.get("label"), str) and "textEdit" in result and "command" not in result:
        result["command"] = {"title": "Resolved completion", "command": ""}
    return message


def forward_server(stream, engine="intellij"):
    """Preserve server edits while adapting completion metadata for older clients."""
    while True:
        headers = {}
        while True:
            line = stream.readline()
            if not line:
                return
            if line in [b"\r\n", b"\n"]:
                break
            name, value = line.split(b":", 1)
            headers[name.lower()] = value.strip()
        size = int(headers[b"content-length"])
        if not 0 <= size <= 64 * 1024 * 1024:
            raise ValueError("Invalid language-server message size")
        data = stream.read(size)
        if len(data) != size:
            raise ValueError("Incomplete language-server message")
        message = json.loads(data)
        message = normalize_spring_completion(message) if engine == "spring" else normalize_completion_filters(message)
        data = json.dumps(message, ensure_ascii=False, separators=(",", ":")).encode()
        sys.stdout.buffer.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
        sys.stdout.buffer.flush()


def run(path):
    spec = json.loads(pathlib.Path(path).read_text())
    env = dict(os.environ)
    command = list(spec["command"])
    lease = None
    if spec["engine"] == "intellij":
        eula = pathlib.Path(spec["eula"])
        sha = hashlib.sha256(eula.read_bytes()).hexdigest()
        if not spec.get("eulaAccepted") or sha != spec["eulaSha256"]:
            raise ValueError("Read and accept this bundle's EULA before enabling IntelliJ; the license content must match")
        cache = pathlib.Path(spec["cache"])
        cache.mkdir(parents=True, exist_ok=True)
        for slot in range(8):
            candidate = (cache / f"slot-{slot}.lock").open("a")
            try:
                fcntl.flock(candidate, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                candidate.close()
                continue
            lease = candidate
            command.extend(["--stdio", "--system-path", str(cache / f"slot-{slot}"), "--eula", sha[:16]])
            break
        if lease is None:
            raise ValueError("All eight IntelliJ index slots for this project are in use")
        for key in ["IJ_LAUNCHER_DEBUG", "INTELLIJ_DATA_SHARING", "INTELLIJ_REGION"]:
            env.pop(key, None)
        env["INTELLIJ_REGION"] = spec["region"]
        if spec.get("mavenHome"):
            env["PATH"] = str(pathlib.Path(spec["mavenHome"]) / "bin") + os.pathsep + env.get("PATH", "")
        if spec.get("importJavaHome"):
            env["JAVA_HOME"] = spec["importJavaHome"]
    try:
        normalize = spec["engine"] in ["intellij", "spring"]
        child = subprocess.Popen(command, cwd=spec["project"], env=env, stdout=subprocess.PIPE if normalize else None)
        previous = {}
        def forward(signum, _frame):
            if child.poll() is None:
                child.send_signal(signum)
        for signum in [signal.SIGTERM, signal.SIGINT]:
            previous[signum] = signal.signal(signum, forward)
        try:
            if normalize:
                forward_server(child.stdout, spec["engine"])
            return child.wait()
        finally:
            for signum, handler in previous.items():
                signal.signal(signum, handler)
    finally:
        if lease is not None:
            lease.close()


if __name__ == "__main__":
    try:
        sys.exit(run(sys.argv[1]))
    except Exception as error:
        print(f"Language service: {error}", file=sys.stderr)
        sys.exit(78)
