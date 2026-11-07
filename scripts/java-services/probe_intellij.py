#!/usr/bin/env python3
"""Test a user-approved IntelliJ server against the existing Java 8 demo."""
import argparse
import asyncio
import json
import pathlib
import sys
from probe import capabilities, start


async def run(args):
    root = args.project.resolve(strict=True)
    spec = json.loads(args.spec.read_text())
    if not spec.get("eulaAccepted"):
        raise RuntimeError("The user must accept the bundle's EULA before this test")
    args.output.mkdir(parents=True, exist_ok=True)
    launcher = pathlib.Path(__file__).with_name("launch.py").resolve()
    client = await start([sys.executable, launcher, args.spec.resolve()], root, args.output / "intellij.log")
    try:
        params = {"processId": None, "rootUri": root.as_uri(), "capabilities": capabilities(),
                  "workspaceFolders": [{"uri": root.as_uri(), "name": root.name}],
                  "initializationOptions": {"buildTools": {root.as_uri(): "maven"},
                      "defaultSdk": str(args.project_jdk.resolve()), "intellijExtensions": False,
                      "lazyIntentions": False, "runMainCodeLens": False}}
        initial = await client.request("initialize", params, timeout=180)
        await client.notify("initialized", {})
        license_status = None
        if initial.get("capabilities", {}).get("experimental", {}).get("licensing"):
            state = await client.request("jetbrains/licensing/state/get", None, timeout=15)
            license_status = (state.get("activeLicense") or {}).get("status")
            if license_status != "Valid" and args.start_trial:
                await client.request("jetbrains/licensing/trial/startAnonymous", None, timeout=65)
                state = await client.request("jetbrains/licensing/state/get", None, timeout=15)
                license_status = (state.get("activeLicense") or {}).get("status")
            if license_status != "Valid":
                raise RuntimeError("IntelliJ requires a valid license or trial; no feature result is claimed")
        source = root / "src/main/java/com/example/demo/UserService.java"
        text = source.read_text()
        line = next((i for i, value in enumerate(text.splitlines()) if "return userMapper.findById(id);" in value), None)
        if line is None:
            raise RuntimeError("Restore the demo's complete UserService.findById call before probing")
        column = text.splitlines()[line].index("userMapper.") + len("userMapper.")
        await client.notify("textDocument/didOpen", {"textDocument": {"uri": source.as_uri(), "languageId": "java", "version": 1, "text": text}})
        expected, items = "findById", []
        for attempt in range(90):
            try:
                result = await client.request("textDocument/completion", {"textDocument": {"uri": source.as_uri()},
                    "position": {"line": line, "character": column}, "context": {"triggerKind": 1}}, timeout=30)
                items = result if isinstance(result, list) else (result or {}).get("items", [])
                if any(expected in i.get("label", "") for i in items):
                    break
            except RuntimeError:
                pass
            await asyncio.sleep(2)
        else:
            raise RuntimeError("IntelliJ did not return Mapper completion; inspect local server/import/license logs")
        print("PASS IntelliJ Java 8: userMapper. -> findById", flush=True)
        locations = await client.request("textDocument/definition", {"textDocument": {"uri": source.as_uri()},
            "position": {"line": line, "character": column + 2}})
        locations = locations if isinstance(locations, list) else [locations]
        if not any("UserMapper.java" in (loc or {}).get("uri", (loc or {}).get("targetUri", "")) for loc in locations):
            raise RuntimeError("IntelliJ definition did not target UserMapper.java")
        print("PASS IntelliJ navigation: UserService -> UserMapper.java", flush=True)
        fresh = text.replace("return userMapper.findById(id);", "return userMapper.f;")
        await client.notify("textDocument/didChange", {"textDocument": {"uri": source.as_uri(), "version": 2}, "contentChanges": [{"text": fresh}]})
        fresh_labels = []
        for attempt in range(10):
            result = await client.request("textDocument/completion", {"textDocument": {"uri": source.as_uri()},
                "position": {"line": line, "character": column + 1}, "context": {"triggerKind": 1}})
            candidates = result if isinstance(result, list) else (result or {}).get("items", [])
            fresh_labels = [i.get("label", "") for i in candidates]
            if any(expected in label for label in fresh_labels):
                break
            await asyncio.sleep(1)
        (args.output / "fresh-prefix.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
        if not any(expected in label for label in fresh_labels):
            raise RuntimeError("Fresh method prefix did not complete; existing-token completion alone is insufficient")
        print("PASS IntelliJ fresh prefix: userMapper.f -> findById", flush=True)
        report = {"project": str(root), "projectJdk": str(args.project_jdk), "serverInfo": initial.get("serverInfo"), "licenseStatus": license_status,
                  "completion": [i["label"] for i in items if expected in i.get("label", "")],
                  "freshCompletion": fresh_labels,
                  "definition": locations, "scope": "LSP protocol test; editor UI and advanced Spring features are not verified"}
        (args.output / "result.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
        await client.notify("textDocument/didClose", {"textDocument": {"uri": source.as_uri()}})
    finally:
        await client.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["project", "project-jdk", "spec", "output"]:
        parser.add_argument("--" + name, type=pathlib.Path, required=True)
    parser.add_argument("--start-trial", action="store_true", help="Only with the user's explicit authorization to start the trial")
    try:
        asyncio.run(run(parser.parse_args()))
    except Exception as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)
