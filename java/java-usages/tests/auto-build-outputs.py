"""Run the Maven reactor save regression with Java 25 on PATH.
python3 auto-build-outputs.py /path/to/jdtls [/path/to/maven-mirror]
"""
import itertools
import json
import os
import pathlib
import platform
import queue
import subprocess
import sys
import tempfile
import threading
import time
from xml.sax.saxutils import escape

jdtls = pathlib.Path(sys.argv[1]).resolve()
bundle = pathlib.Path(__file__).resolve().parents[1] / "openidea-java-usages.jar"
work = pathlib.Path(tempfile.mkdtemp(prefix="openidea-auto-build-")).resolve()
project = work / "project"
print(f"Workspace and logs: {work}", flush=True)


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


def pom(module, body):
    return write(project / module / "pom.xml", f"<project><modelVersion>4.0.0</modelVersion>{body}</project>")


coordinates = "<groupId>demo</groupId><artifactId>reactor</artifactId><version>1.0</version>"
pom("", f"""
{coordinates}
<packaging>pom</packaging><modules><module>lib</module><module>app</module></modules>
<properties><maven.compiler.release>21</maven.compiler.release></properties>
<build><plugins>
  <plugin><artifactId>maven-clean-plugin</artifactId><version>3.3.2</version></plugin>
  <plugin><artifactId>maven-resources-plugin</artifactId><version>3.3.1</version></plugin>
  <plugin><artifactId>maven-compiler-plugin</artifactId><version>3.13.0</version></plugin>
</plugins></build>
""")
pom("lib", f"<parent>{coordinates}</parent><artifactId>lib</artifactId>"
    "<build><outputDirectory>${project.build.directory}/library-classes</outputDirectory></build>")
pom("app", f"<parent>{coordinates}</parent><artifactId>app</artifactId>"
    "<dependencies><dependency><groupId>demo</groupId><artifactId>lib</artifactId>"
    "<version>1.0</version></dependency></dependencies>")
lib_text = """package demo; public class Marker {
    public static String value() { return "good"; }
    static class Previous {}
}"""
app_text = 'package demo; public class Main { public static void main(String[] args) { System.out.println(Marker.value()); } }'
lib = write(project / "lib/src/main/java/demo/Marker.java", lib_text)
app = write(project / "app/src/main/java/demo/Main.java", app_text)
lib_output = project / "lib/target/library-classes"
app_output = project / "app/target/classes"
artifacts = [lib_output / "demo/Marker.class", app_output / "demo/Main.class"]
removed = lib_output / "demo/Marker$Previous.class"
added = lib_output / "demo/Marker$Added.class"

mirror = ""
if len(sys.argv) > 2:
    uri = pathlib.Path(sys.argv[2]).resolve().as_uri()
    mirror = f"<mirrors><mirror><id>fixture-cache</id><mirrorOf>*</mirrorOf><url>{uri}</url></mirror></mirrors>"
settings_file = write(work / "settings.xml",
                      f"<settings><localRepository>{escape(str(work / 'repository'))}</localRepository>{mirror}</settings>")
settings = {"java": {"autobuild": {"enabled": True}, "import": {"gradle": {"enabled": False}},
                     "configuration": {"maven": {"userSettings": str(settings_file)}}}}

config_name = {"Darwin": "config_mac", "Windows": "config_win"}.get(platform.system(), "config_linux")
config_names = ([config_name + "_arm"] if platform.machine().lower() in ("arm64", "aarch64") else []) + [config_name]
config = next(jdtls / name for name in config_names if (jdtls / name / "config.ini").is_file())
(work / "home").mkdir()
with (work / "stderr.log").open("wb") as stderr:
    server = subprocess.Popen([
        "java", f"-Duser.home={work / 'home'}", "-Declipse.application=org.eclipse.jdt.ls.core.id1",
        "-Dosgi.bundles.defaultStartLevel=4", "-Declipse.product=org.eclipse.jdt.ls.core.product",
        f"-Dosgi.sharedConfiguration.area={config}", "-Dosgi.sharedConfiguration.area.readOnly=true",
        "-Dosgi.configuration.cascaded=true", f"-Dosgi.configuration.area={work / 'config'}",
        "--add-modules=ALL-SYSTEM", "--add-opens=java.base/java.util=ALL-UNNAMED",
        "--add-opens=java.base/java.lang=ALL-UNNAMED", "-DDetectVMInstallationsJob.disabled=true",
        "-Xmx512m", "-jar", str(next((jdtls / "plugins").glob("org.eclipse.equinox.launcher_*.jar"))),
        "-data", str(work / "data"),
    ], cwd=project, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr)

messages = queue.Queue()
ids = itertools.count(1)


def read():
    try:
        while True:
            headers = {}
            while (line := server.stdout.readline()) not in (b"", b"\r\n"):
                name, value = line.split(b":", 1)
                headers[name.lower()] = value.strip()
            if not line:
                return
            messages.put(json.loads(server.stdout.read(int(headers[b"content-length"]))))
    finally:
        messages.put(None)


def send(method=None, params=None, **fields):
    if method is not None:
        fields.update(method=method, params=params)
    data = json.dumps(dict(jsonrpc="2.0", **fields)).encode()
    server.stdin.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
    server.stdin.flush()


def wait_for(match, timeout=90):
    deadline = time.monotonic() + timeout
    with (work / "rpc.jsonl").open("a", encoding="utf-8") as transcript:
        while True:
            msg = messages.get(timeout=max(0, deadline - time.monotonic()))
            if msg is None:
                raise RuntimeError(f"JDT LS exited. See {work / 'stderr.log'}")
            print(json.dumps(msg), file=transcript)
            if "method" in msg and "id" in msg:
                send(id=msg["id"], result=None)
            elif match(msg):
                return msg


def request(method, params):
    number = next(ids)
    send(method, params, id=number)
    reply = wait_for(lambda msg: msg.get("id") == number)
    assert "error" not in reply, reply
    return reply.get("result")


def save(path, contents, version):
    send("textDocument/didChange", {"textDocument": {"uri": path.as_uri(), "version": version},
                                    "contentChanges": [{"text": contents}]})
    write(path, contents)
    send("textDocument/didSave", {"textDocument": {"uri": path.as_uri()}, "text": contents})
    send("workspace/didChangeWatchedFiles", {"changes": [{"uri": path.as_uri(), "type": 2}]})


def diagnostic(msg, path, errors, error_text=""):
    params = msg.get("params", {})
    return (msg.get("method") == "textDocument/publishDiagnostics" and params.get("uri") == path.as_uri()
            and any(item["severity"] == 1 and error_text in item["message"]
                    for item in params["diagnostics"]) == errors)


def output():
    run = subprocess.run(["java", "-cp", os.pathsep.join(map(str, [app_output, lib_output])), "demo.Main"],
                         text=True, capture_output=True, timeout=10)
    assert run.returncode == 0, run.stderr
    return run.stdout.strip()


def wait_output(expected):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        try:
            if output() == expected:
                return
        except AssertionError:
            pass  # The JVM can read between the two modules' writes during a successful build.
        time.sleep(.15)
    assert output() == expected, expected


threading.Thread(target=read, daemon=True).start()
try:
    request("initialize", {
        "processId": os.getpid(), "rootUri": project.as_uri(), "capabilities": {},
        "workspaceFolders": [{"uri": project.as_uri(), "name": "reactor"}],
        "initializationOptions": {"bundles": [str(bundle)], "workspaceFolders": [project.as_uri()], "settings": settings},
    })
    send("initialized", {})
    wait_for(lambda msg: msg.get("method") == "language/status" and msg["params"]["type"] == "ServiceReady")
    assert request("workspace/executeCommand", {"command": "openidea.java.build", "arguments": []}) == 1
    before = [path.read_bytes() for path in artifacts]
    removed_before = removed.read_bytes()
    assert output() == "good"
    send("textDocument/didOpen", {"textDocument": {
        "uri": lib.as_uri(), "languageId": "java", "version": 1, "text": lib_text,
    }})

    changed_lib = lib_text.replace("String value()", "String renamed()").replace("class Previous", "class Added")
    save(lib, changed_lib, 2)
    # The downstream buffer is closed, so the error comes from the workspace build.
    wait_for(lambda msg: diagnostic(msg, app, True))
    assert [path.read_bytes() for path in artifacts] == before
    assert removed.read_bytes() == removed_before and not added.exists()
    assert output() == "good"
    print("Failed save preserved both outputs and published downstream diagnostics", flush=True)

    # Roll back again while Eclipse's resource tree still reflects the first failure.
    write(app, app_text.replace("Marker.value()", "Marker.stillMissing()"))
    send("workspace/didChangeWatchedFiles", {"changes": [{"uri": app.as_uri(), "type": 2}]})
    wait_for(lambda msg: diagnostic(msg, app, True, "stillMissing"))
    assert [path.read_bytes() for path in artifacts] == before
    assert removed.read_bytes() == removed_before and not added.exists()
    assert output() == "good"
    print("Second failed save preserved the restored class files", flush=True)

    # Keep the breaking upstream edit. Saving only the downstream caller must rebuild both.
    write(app, app_text.replace("Marker.value()", "Marker.renamed()"))
    send("workspace/didChangeWatchedFiles", {"changes": [{"uri": app.as_uri(), "type": 2}]})
    wait_for(lambda msg: diagnostic(msg, app, False))
    assert all(path.read_bytes() != original for path, original in zip(artifacts, before))
    assert added.is_file() and not removed.exists()
    assert output() == "good"

    save(lib, changed_lib.replace('"good"', '"repaired"'), 3)
    wait_output("repaired")
    print("Downstream-only repair and subsequent upstream save both reached the external JVM", flush=True)

    # An explicit retry can fail between an automatic failure and the repair save.
    before = [path.read_bytes() for path in artifacts]
    mixed_lib = changed_lib.replace("renamed()", "renamedAgain()").replace('"good"', '"mixed-repaired"')
    save(lib, mixed_lib, 4)
    wait_for(lambda msg: diagnostic(msg, app, True, "renamed"))
    assert request("workspace/executeCommand", {"command": "openidea.java.build", "arguments": []}) == 2
    assert [path.read_bytes() for path in artifacts] == before and output() == "repaired"

    write(app, app_text.replace("Marker.value()", "Marker.renamedAgain()"))
    send("workspace/didChangeWatchedFiles", {"changes": [{"uri": app.as_uri(), "type": 2}]})
    wait_for(lambda msg: diagnostic(msg, app, False))
    wait_output("mixed-repaired")
    print("Downstream-only save recovered after a failed explicit retry", flush=True)
    request("shutdown", None)
    send("exit", None)
finally:
    server.stdin.close()
    try:
        server.wait(timeout=10)
    except subprocess.TimeoutExpired:
        server.kill()
        server.wait()
