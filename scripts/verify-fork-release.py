#!/usr/bin/env python3
"""Verify the personal Mac release manifest and required bundled defaults."""
import argparse
import json
import pathlib
import subprocess
import tarfile
import urllib.parse


def verify(tag, output):
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run(["gh", "release", "download", tag, "--repo", "hyhanyue/openidea", "--dir", str(output),
        "--pattern", "latest.json", "--pattern", "*.app.tar.gz", "--pattern", "*.app.tar.gz.sig", "--clobber"], check=True)
    manifest = json.loads((output / "latest.json").read_text())
    assert manifest["version"] == tag.removeprefix("v"), "Manifest version differs from release tag"
    item = manifest["platforms"]["darwin-aarch64"]
    base = f"https://github.com/hyhanyue/openidea/releases/download/{tag}/"
    assert item["url"].startswith(base), "Update URL leaves this fork/release"
    name = urllib.parse.unquote(item["url"].removeprefix(base))
    assert pathlib.PurePath(name).name == name, "Invalid asset name"
    assert item["signature"].strip() == (output / (name + ".sig")).read_text().strip(), "Updater signature differs from asset"
    with tarfile.open(output / name) as bundle:
        names = bundle.getnames()
        required = ["fork-defaults/manifest.json", "fork-defaults/plugins/mybatis/dist/plugin.wasm",
            "fork-defaults/plugins/spring-beans/dist/plugin.wasm", "fork-defaults/bin/launch.py",
            "fork-defaults/spring-tools/extension/language-server/spring-boot-language-server-2.3.0-SNAPSHOT-exec.jar"]
        for suffix in required:
            assert any(n.endswith(suffix) for n in names), f"Missing bundled default: {suffix}"
        assert not any("intellij-server-2026" in n or "slot-0" in n for n in names), "Personal IntelliJ runtime/cache in public package"
    print(f"PASS {tag}: signed manifest targets this fork; default plugins and Spring runtime are bundled")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("--download-dir", type=pathlib.Path, required=True)
    args = parser.parse_args()
    verify(args.tag, args.download_dir)
