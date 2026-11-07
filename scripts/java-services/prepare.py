#!/usr/bin/env python3
"""Prepare optional language-server settings; never edits the project's settings."""
import argparse
import hashlib
import json
import pathlib
import sys
import tomllib


def quoted(value):
    return json.dumps(str(value), ensure_ascii=False)


def prepare(args):
    root = args.project.resolve(strict=True)
    jdk = args.project_jdk.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=True)
    out = args.output.resolve()
    spec_path = out / "launch.json"
    settings_path = out / "settings.fragment.toml"
    if spec_path.exists() or settings_path.exists():
        raise ValueError("Output already contains a configuration; choose a new output directory")
    launcher = pathlib.Path(__file__).with_name("launch.py").resolve()
    common = f'[languages.java]\njdk = {quoted(jdk)}\n\n'
    spec = {"engine": args.engine, "project": str(root)}
    if args.engine == "spring":
        if not args.spring or not args.java_home:
            raise ValueError("Spring requires --spring and --java-home")
        home = args.spring.resolve(strict=True)
        runtime = args.java_home.resolve(strict=True)
        server = next((home / "language-server").glob("*-exec.jar"))
        bundles = sorted(str(p) for p in (home / "jars").glob("*.jar"))
        if not bundles:
            raise ValueError("Official Spring JDT extension bundles are missing")
        spec["command"] = [str(runtime / "bin/java"), "-Dspring.config.location=classpath:/application.properties",
                           "-Dserver.port=-1", "-Xmx1g", "-jar", str(server)]
        # Existing OpenIDEA usages/debugger bundles are merged, not replaced, by its settings hook.
        common += '[languageServers.jdtls]\nenabled = true\n\n'
        common += '[languageServers.jdtls.initializationOptions]\nbundles = ' + json.dumps(bundles, ensure_ascii=False) + '\n\n'
        common += '[languageServers.spring-boot]\nenabled = true\n'
        common += f'command = {quoted(sys.executable)}\nargs = [{quoted(launcher)}, {quoted(spec_path)}]\n'
        common += 'languages = ["java", "properties", "yaml"]\nrootMarkers = ["pom.xml", "build.gradle", "build.gradle.kts"]\n'
        common += 'features = ["completion", "hover", "definition", "references", "code-action", "diagnostics"]\n\n'
        common += '[languageServers.spring-boot.initializationOptions]\nenableJdtClasspath = true\n'
        common += f'workspaceFolders = [{quoted(root.as_uri())}]\n'
    else:
        if not args.server:
            raise ValueError("IntelliJ requires --server")
        server = args.server.resolve(strict=True)
        eula = server.parent.parent / "EULA.txt"
        if not eula.is_file():
            raise ValueError("Expected an official bundle containing EULA.txt")
        sha = hashlib.sha256(eula.read_bytes()).hexdigest()
        spec.update({"command": [str(server)], "eula": str(eula), "eulaSha256": sha,
                     "eulaAccepted": args.accept_eula, "region": args.region})
        spec["cache"] = str(out / "index")
        maven = args.maven_home
        if maven is None and (root / ".openidea/settings.toml").is_file():
            settings = tomllib.loads((root / ".openidea/settings.toml").read_text())
            configured = settings.get("languages", {}).get("java", {}).get("maven", {}).get("home")
            if configured:
                maven = pathlib.Path(configured)
        if maven:
            spec["mavenHome"] = str(maven.resolve(strict=True))
        if args.java_home:
            spec["importJavaHome"] = str(args.java_home.resolve(strict=True))
        # Keep JDT available for OpenIDEA's existing build/run/debug commands.
        # It contributes no duplicate completion, hover or diagnostics in this preset.
        common += '[languageServers.jdtls]\nenabled = true\n'
        if args.accept_eula:
            common += 'features = ["code-lens"]\n'
        common += '\n'
        common += '[languageServers.intellij]\nenabled = ' + str(args.accept_eula).lower() + '\n'
        common += f'command = {quoted(sys.executable)}\nargs = [{quoted(launcher)}, {quoted(spec_path)}]\n'
        common += 'languages = ["java"]\nrootMarkers = ["pom.xml", "build.gradle", "build.gradle.kts"]\n\n'
        common += '[languageServers.intellij.initializationOptions]\n'
        common += f'defaultSdk = {quoted(jdk)}\nintellijExtensions = false\nlazyIntentions = false\nrunMainCodeLens = false\n'
        common += '[languageServers.intellij.initializationOptions.buildTools]\n'
        common += f'{quoted(root.as_uri())} = "maven"\n'
        (out / "CONSENT.txt").write_text(
            f'Official bundle license: {eula}\nSHA-256: {sha}\nAccepted: {args.accept_eula}\n'
            'This tool does not activate, purchase or supply an Ultimate subscription.\n')
    spec_path.write_text(json.dumps(spec, ensure_ascii=False, indent=2) + "\n")
    settings_path.write_text(common)
    print(settings_path)
    if args.engine == "intellij" and not args.accept_eula:
        print("IntelliJ remains disabled until you read and accept its EULA; no subscription has been activated.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", choices=["spring", "intellij"], required=True)
    for name in ["project", "project-jdk", "output"]:
        parser.add_argument("--" + name, type=pathlib.Path, required=True)
    for name in ["spring", "java-home", "server", "maven-home"]:
        parser.add_argument("--" + name, type=pathlib.Path)
    parser.add_argument("--accept-eula", action="store_true", help="Only after the user reads and accepts this bundle's EULA")
    parser.add_argument("--region", choices=["africa", "americas", "apac", "china", "europe", "middle_east", "oceania"], default="china")
    prepare(parser.parse_args())
