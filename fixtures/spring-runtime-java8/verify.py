#!/usr/bin/env python3
"""Verify real Actuator responses without saving bodies or credentials."""
import os
from pathlib import Path
import secrets
import socket
import subprocess
import time
import urllib.error
import urllib.request

fixture = Path(__file__).resolve().parent
repo = fixture.parents[1]
java = str(Path(os.environ["JAVA_HOME"]) / "bin/java")
cargo = os.environ.get("CARGO", "cargo")
jar = fixture / "target/spring-runtime-java8-1.0.0.jar"

def verify(secured):
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    env = dict(os.environ)
    env["FIXTURE_TEST_USER"] = secrets.token_hex(12)
    env["FIXTURE_TEST_PASSWORD"] = secrets.token_hex(24)
    env["SPRING_RUNTIME_FIXTURE_URL"] = "http://127.0.0.1:%d/actuator" % port
    args = [java, "-jar", str(jar), "--server.port=%d" % port, "--spring.main.banner-mode=off"]
    if secured:
        args += ["--fixture.secured=true", "--management.endpoint.env.enabled=false"]
    app = subprocess.Popen(args, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        for _ in range(150):
            if app.poll() is not None:
                raise RuntimeError("Fixture exited before readiness")
            try:
                response = opener.open(env["SPRING_RUNTIME_FIXTURE_URL"] + "/beans", timeout=0.5)
                response.close()
                break
            except urllib.error.HTTPError as error:
                code = error.code
                error.close()
                if code == 401 and secured:
                    break
            except (OSError, urllib.error.URLError):
                pass
            time.sleep(0.1)
        else:
            raise RuntimeError("Fixture readiness timed out")
        test = ("spring_runtime::fixture_error_tests::real_java8_security_and_disabled_endpoint"
                if secured else "spring_runtime::tests::real_java8_fixture")
        subprocess.run([cargo, "test", "-p", "ide-http", "--lib", test, "--offline", "--", "--ignored", "--exact"],
                       cwd=repo, env=env, check=True)
    finally:
        app.terminate()
        try:
            app.wait(timeout=8)
        except subprocess.TimeoutExpired:
            app.kill()
            app.wait()

if __name__ == "__main__":
    verify(False)
    verify(True)
    print("Java 8 / Boot 2.7.18: real responses, redaction, 401, 403 and disabled endpoint verified; fixtures stopped.")
