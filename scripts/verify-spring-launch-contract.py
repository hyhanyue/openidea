#!/usr/bin/env python3
"""Test a consumer source snapshot against the frozen producer payloads in an isolated copy."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--consumer-root", type=Path, required=True)
parser.add_argument("--work-dir", type=Path, required=True)
parser.add_argument("--cargo", default="cargo")
args = parser.parse_args()
repo = Path(__file__).resolve().parents[1]
consumer = args.consumer_root.resolve()
work = args.work_dir.resolve()
if work == consumer or consumer in work.parents:
    raise SystemExit("work directory must be outside the consumer source checkout")
work.mkdir(parents=True, exist_ok=True)
source = work / "source"
if source.is_symlink():
    raise SystemExit("refusing a symlink source directory")
marker = work / ".spring-launch-contract-test"
if source.exists():
    previous = work / "manifest.json"
    owned = marker.is_file() or previous.is_file() and json.loads(previous.read_text()).get("consumerRoot") == str(consumer)
    if not owned:
        raise SystemExit("refusing to replace an unowned source directory")
    shutil.rmtree(source)
marker.write_text("openidea.spring-launch-context v1 isolated test\n")
source.mkdir()
paths = ["plugins/official/spring-beans", "plugins/support/spring-text", "sdk/rust/openidea-plugin", "fixtures/java-spring-configuration"]
files = {}
for relative in paths:
    original = consumer / relative
    destination = source / relative
    shutil.copytree(original, destination, ignore=shutil.ignore_patterns("target", "dist", ".git", "icons"))
    for path in destination.rglob("*"):
        if path.is_file():
            files[str(path.relative_to(source))] = hashlib.sha256(path.read_bytes()).hexdigest()
contract = repo / "docs/spring-launch-context-contract.json"
plugin = source / "plugins/official/spring-beans"
(plugin / "f-launch-contract.json").write_bytes(contract.read_bytes())
with (plugin / "src/environment.rs").open("a") as stream:
    stream.write("\n" + (repo / "scripts/spring-launch-contract-consumer.rs").read_text())
manifest = {"consumerRoot": str(consumer), "contractSha256": hashlib.sha256(contract.read_bytes()).hexdigest(), "sourceHashes": files}
(work / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
results = []
for test_filter in ["f_launch_contract", "launch::tests"]:
    result = subprocess.run([args.cargo, "test", "--manifest-path", str(plugin / "Cargo.toml"), "--target-dir", str(work / "target"), "--offline", "--lib", test_filter, "--", "--nocapture"], check=False)
    results.append({"filter": test_filter, "exitCode": result.returncode})
manifest["results"] = results
manifest["exitCode"] = next((result["exitCode"] for result in results if result["exitCode"]), 0)
manifest["sourceStillMatches"] = all((consumer / path).is_file() and hashlib.sha256((consumer / path).read_bytes()).hexdigest() == digest for path, digest in files.items())
(work / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
raise SystemExit(manifest["exitCode"])
