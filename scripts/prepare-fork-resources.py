#!/usr/bin/env python3
"""Stage this fork's default plugins and pinned official Spring Tools for a release."""
import argparse
import hashlib
import json
import pathlib
import shutil
import tempfile
import urllib.request
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
VERSION = "2.3.0"
SHA256 = "8e555da123e5b4edb7449d3ef1f922a922503e64a86cd66cbe713638f94a9e50"
URL = "https://open-vsx.org/api/VMware/vscode-spring-boot/2.3.0/file/VMware.vscode-spring-boot-2.3.0.vsix"


def prepare(vsix=None):
    destination = ROOT / "src-tauri/resources/fork-defaults"
    destination.mkdir(parents=True, exist_ok=True)
    spring = destination / "spring-tools/extension"
    if not (spring / "language-server/spring-boot-language-server-2.3.0-SNAPSHOT-exec.jar").is_file():
        archive = pathlib.Path(vsix) if vsix else ROOT / "target/fork-resources/spring-tools-2.3.0.vsix"
        if not archive.exists():
            archive.parent.mkdir(parents=True, exist_ok=True)
            with urllib.request.urlopen(URL, timeout=120) as response, archive.open("wb") as target:
                shutil.copyfileobj(response, target)
        if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
            raise ValueError("Spring Tools SHA-256 mismatch")
        with tempfile.TemporaryDirectory(dir=destination) as temporary:
            extracted = pathlib.Path(temporary)
            with zipfile.ZipFile(archive) as bundle:
                for entry in bundle.infolist():
                    path = pathlib.PurePosixPath(entry.filename)
                    if not entry.is_dir() and path.parts[0] == "extension" and ".." not in path.parts:
                        target = extracted.joinpath(*path.parts)
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(bundle.read(entry))
            manifest = json.loads((extracted / "extension/package.json").read_text())
            if manifest["version"] != VERSION:
                raise ValueError("Unexpected Spring Tools version")
            spring.parent.mkdir(exist_ok=True)
            if spring.exists(): shutil.rmtree(spring)
            shutil.move(str(extracted / "extension"), spring)
    app_version = json.loads((ROOT / "package.json").read_text())["version"]
    package_version = "0.2.0+openidea." + app_version
    packages = {}
    for name, plugin_id in [("mybatis", "hyhanyue.mybatis"), ("spring-beans", "hyhanyue.spring-beans")]:
        source = ROOT / "plugins/official" / name
        target = destination / "plugins" / name
        if target.exists(): shutil.rmtree(target)
        target.mkdir(parents=True)
        for item in source.iterdir():
            if item.name in ["dist", "icons"]:
                shutil.copytree(item, target / item.name)
            elif item.name in ["plugin.toml", "README.md", "LICENSE", "NOTICE", "UPSTREAM.md", "settings.schema.json"]:
                shutil.copy2(item, target / item.name)
        path = target / "plugin.toml"
        lines = path.read_text().splitlines()
        lines = [f'id = "{plugin_id}"' if line.startswith("id = ") and i == 0 else
                 f'version = "{package_version}"' if line.startswith("version = ") else
                 'engines.openidea = "^1.3"' if line.startswith("engines.openidea = ") else line for i, line in enumerate(lines)]
        path.write_text("\n".join(lines) + "\n")
        packages[name] = {"id": plugin_id, "version": package_version}
    launcher = destination / "bin/launch.py"
    launcher.parent.mkdir(exist_ok=True)
    shutil.copy2(ROOT / "scripts/java-services/launch.py", launcher)
    (destination / "manifest.json").write_text(json.dumps({"format": 1, "plugins": packages, "springTools": VERSION,
        "springToolsUrl": URL, "springToolsSha256": SHA256,
        "source": "https://github.com/spring-projects/spring-tools"}, indent=2) + "\n")
    print(f"Prepared default plugins and Spring Tools {VERSION}: {destination}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--spring-vsix", type=pathlib.Path)
    prepare(parser.parse_args().spring_vsix)
