#!/usr/bin/env python3
"""Probe official Spring Tools against a Java 8 Maven project without editing source files."""
import argparse
import asyncio
import json
import pathlib
import platform
import shutil
import sys
import tempfile

JAVA_REQUESTS = {
    "sts/javaType": "sts.java.type", "sts/javadocHoverLink": "sts.java.javadocHoverLink",
    "sts/javaLocation": "sts.java.location", "sts/javadoc": "sts.java.javadoc",
    "sts/javaSearchTypes": "sts.java.search.types", "sts/javaSearchPackages": "sts.java.search.packages",
    "sts/javaSubTypes": "sts.java.hierarchy.subtypes", "sts/javaSuperTypes": "sts.java.hierarchy.supertypes",
    "sts/javaCodeComplete": "sts.java.code.completions", "sts/project/gav": "sts.project.gav",
}


class Client:
    def __init__(self, process, log):
        self.process, self.log = process, log
        self.pending, self.next_id, self.handlers = {}, 0, {}
        self.lock = asyncio.Lock()
        self.tasks = set()
        self.ready = asyncio.Event()
        self.reader = asyncio.create_task(self.read())
        self.stderr = asyncio.create_task(self.read_stderr())

    async def send(self, message):
        data = json.dumps({"jsonrpc": "2.0", **message}, ensure_ascii=False).encode()
        async with self.lock:
            self.process.stdin.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
            await self.process.stdin.drain()

    async def request(self, method, params, timeout=120):
        self.next_id += 1
        key = self.next_id
        future = asyncio.get_running_loop().create_future()
        self.pending[key] = future
        try:
            await self.send({"id": key, "method": method, "params": params})
            return await asyncio.wait_for(future, timeout)
        finally:
            self.pending.pop(key, None)

    async def notify(self, method, params):
        await self.send({"method": method, "params": params})

    async def answer(self, message):
        method, params = message["method"], message.get("params", {})
        try:
            if method in self.handlers:
                result = await self.handlers[method](params)
            elif method == "workspace/configuration":
                result = [{} for _ in params.get("items", [])]
            elif method in ["client/registerCapability", "client/unregisterCapability", "window/workDoneProgress/create"]:
                result = None
            elif method in ["window/showMessageRequest", "workspace/semanticTokens/refresh", "workspace/codeLens/refresh"]:
                result = None
            else:
                await self.send({"id": message["id"], "error": {"code": -32601, "message": method}})
                return
            await self.send({"id": message["id"], "result": result})
        except Exception as error:
            await self.send({"id": message["id"], "error": {"code": -32603, "message": str(error)}})

    async def read(self):
        try:
            while True:
                header = await self.process.stdout.readuntil(b"\r\n\r\n")
                length = next(int(line.split(b":", 1)[1]) for line in header.split(b"\r\n") if line.lower().startswith(b"content-length:"))
                message = json.loads(await self.process.stdout.readexactly(length))
                if "method" in message:
                    if "id" in message:
                        task = asyncio.create_task(self.answer(message))
                        self.tasks.add(task)
                        task.add_done_callback(self.tasks.discard)
                    else:
                        if message["method"] == "language/status" and message.get("params", {}).get("type") == "ServiceReady":
                            self.ready.set()
                        self.log.write(json.dumps(message, ensure_ascii=False) + "\n")
                        self.log.flush()
                elif message.get("id") in self.pending:
                    future = self.pending[message["id"]]
                    if not future.done():
                        if "error" in message:
                            future.set_exception(RuntimeError(json.dumps(message["error"])))
                        else:
                            future.set_result(message.get("result"))
        except (asyncio.IncompleteReadError, asyncio.CancelledError):
            pass
        except Exception as error:
            self.log.write(f"reader: {error}\n")
        finally:
            for future in list(self.pending.values()):
                if not future.done():
                    future.set_exception(RuntimeError("Language server connection closed"))

    async def read_stderr(self):
        while line := await self.process.stderr.readline():
            self.log.write(line.decode(errors="replace"))
            self.log.flush()

    async def close(self):
        if self.process.returncode is None:
            try:
                await self.request("shutdown", None, timeout=5)
                await self.notify("exit", None)
                await asyncio.wait_for(self.process.wait(), 5)
            except Exception:
                self.process.terminate()
                try:
                    await asyncio.wait_for(self.process.wait(), 5)
                except asyncio.TimeoutError:
                    self.process.kill()
                    await self.process.wait()
        self.reader.cancel()
        self.stderr.cancel()
        for task in list(self.tasks):
            task.cancel()
        await asyncio.gather(self.reader, self.stderr, *self.tasks, return_exceptions=True)
        self.log.close()


async def start(command, cwd, log):
    process = await asyncio.create_subprocess_exec(*map(str, command), cwd=cwd,
        stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
    return Client(process, log.open("w"))


def capabilities():
    return {"workspace": {"configuration": True, "workspaceFolders": True, "executeCommand": {"dynamicRegistration": False}},
            "textDocument": {"completion": {"completionItem": {"snippetSupport": True}},
                             "hover": {"contentFormat": ["markdown", "plaintext"]}}}


async def run(args):
    root = args.project.resolve()
    if not (root / "pom.xml").is_file():
        raise RuntimeError("Expected an existing Maven project")
    args.output.mkdir(parents=True, exist_ok=True)
    spring_home = args.spring.resolve()
    jars = sorted((spring_home / "jars").glob("*.jar"))
    server_jar = next((spring_home / "language-server").glob("*-exec.jar"))
    launcher = next((args.jdtls / "plugins").glob("org.eclipse.equinox.launcher_*.jar"))
    config = "config_mac_arm" if platform.system() == "Darwin" and platform.machine() == "arm64" else "config_linux"
    work = pathlib.Path(tempfile.mkdtemp(prefix="openidea-spring-probe-"))
    shutil.copytree(args.jdtls / config, work / "configuration")
    java = args.java_home / "bin/java"
    clients = []
    callbacks = set()
    callback_received = asyncio.Event()
    try:
        jdt = await start([java, "-Declipse.application=org.eclipse.jdt.ls.core.id1", "-Dosgi.bundles.defaultStartLevel=4",
            "-Declipse.product=org.eclipse.jdt.ls.core.product", "-Dfile.encoding=UTF-8", "-Xmx1g",
            "--add-modules=ALL-SYSTEM", "--add-opens", "java.base/java.util=ALL-UNNAMED",
            "--add-opens", "java.base/java.lang=ALL-UNNAMED", "-jar", launcher,
            "-configuration", work / "configuration", "-data", work / "workspace"], root, args.output / "jdt.log")
        clients.append(jdt)
        params = {"processId": None, "rootUri": root.as_uri(), "capabilities": capabilities(),
            "workspaceFolders": [{"uri": root.as_uri(), "name": root.name}],
            "initializationOptions": {"bundles": list(map(str, jars)), "workspaceFolders": [root.as_uri()],
                "settings": {"java": {"autobuild": {"enabled": False}, "import": {"gradle": {"enabled": False}, "generatesMetadataFilesAtProjectRoot": False},
                    "maven": {"downloadSources": False}, "configuration": {"updateBuildConfiguration": "automatic", "runtimes": [{"name": "JavaSE-1.8", "path": str(args.project_jdk), "default": True}]}}}}}
        result = await jdt.request("initialize", params)
        commands = result.get("capabilities", {}).get("executeCommandProvider", {}).get("commands", [])
        if "sts.java.addClasspathListener" not in commands:
            raise RuntimeError("Official Spring JDT extension did not load")
        await jdt.notify("initialized", {})
        await asyncio.wait_for(jdt.ready.wait(), 180)
        spring = await start([java, "-Dspring.config.location=classpath:/application.properties", "-Dserver.port=-1", "-Xmx1g", "-jar", server_jar], root, args.output / "spring.log")
        clients.append(spring)

        async def java_request(command, data):
            return await jdt.request("workspace/executeCommand", {"command": command, "arguments": [data]})

        for method, command in JAVA_REQUESTS.items():
            spring.handlers[method] = lambda p, command=command: java_request(command, p)

        async def add_listener(p):
            callbacks.add(p["callbackCommandId"])
            return await java_request("sts.java.addClasspathListener", p["callbackCommandId"])

        async def remove_listener(p):
            callbacks.discard(p["callbackCommandId"])
            return await java_request("sts.java.removeClasspathListener", p["callbackCommandId"])

        async def callback(p):
            if p.get("command") not in callbacks:
                raise RuntimeError("Unregistered JDT callback")
            result = await spring.request("workspace/executeCommand", p)
            callback_received.set()
            return result

        spring.handlers.update({"sts/addClasspathListener": add_listener, "sts/removeClasspathListener": remove_listener})
        jdt.handlers["workspace/executeClientCommand"] = callback
        params["initializationOptions"] = {"workspaceFolders": [root.as_uri()], "enableJdtClasspath": True}
        await spring.request("initialize", params)
        await spring.notify("initialized", {})
        await asyncio.wait_for(callback_received.wait(), 120)
        uri = (root / "src/main/resources/application.properties").as_uri()
        report = {"project": str(root), "projectJava": str(args.project_jdk), "springPackage": spring_home.name, "classpathCallback": True, "checks": []}
        for version, (text, expected) in enumerate([("server.po", "server.port"), ("demo.gre", "demo.greeting")], 1):
            if version == 1:
                await spring.notify("textDocument/didOpen", {"textDocument": {"uri": uri, "languageId": "spring-boot-properties", "version": version, "text": text}})
            else:
                await spring.notify("textDocument/didChange", {"textDocument": {"uri": uri, "version": version}, "contentChanges": [{"text": text}]})
            labels = []
            for attempt in range(30):
                result = await spring.request("textDocument/completion", {"textDocument": {"uri": uri}, "position": {"line": 0, "character": len(text)}, "context": {"triggerKind": 1}})
                items = result if isinstance(result, list) else (result or {}).get("items", [])
                labels = [item["label"] for item in items]
                if expected in labels:
                    break
                await asyncio.sleep(1)
            if expected not in labels:
                raise RuntimeError(f"{text}: missing {expected}, received {labels[:10]}")
            report["checks"].append({"typed": text, "completion": expected})
            print(f"PASS {text} -> {expected}", flush=True)
        (args.output / "result.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    finally:
        for client in reversed(clients):
            await client.close()
        shutil.rmtree(work)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["project", "spring", "jdtls", "java-home", "project-jdk", "output"]:
        parser.add_argument("--" + name, type=pathlib.Path, required=True)
    try:
        asyncio.run(run(parser.parse_args()))
    except Exception as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)
