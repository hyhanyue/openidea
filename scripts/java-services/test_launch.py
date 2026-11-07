import copy
import hashlib
import io
import json
import pathlib
import tempfile
import types
import unittest
from unittest.mock import patch
import launch


class LaunchTests(unittest.TestCase):
    def test_resolved_spring_edit_clears_the_fallback_command_without_changing_the_edit(self):
        resolved = {"label": "server.port", "textEdit": {"range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 9}}, "newText": "server.port="}}
        response = launch.normalize_spring_completion({"id": 1, "result": copy.deepcopy(resolved)})
        self.assertEqual(response["result"]["textEdit"], resolved["textEdit"])
        self.assertEqual(response["result"]["command"]["command"], "")
        self.assertNotIn("command", resolved)

    def test_spring_initial_fallback_and_unrelated_messages_are_preserved(self):
        item = {"label": "server.port", "textEdit": {"newText": "server.port"}, "command": {"command": "sts.vscode-spring-boot.commandList"}}
        for message in [{"result": {"items": [item]}}, {"result": item}, {"method": "workspace/applyEdit", "params": {"edit": {}}}, {"result": None}]:
            self.assertEqual(launch.normalize_spring_completion(copy.deepcopy(message)), message)

    def test_dot_filter_normalization_preserves_the_exact_server_edit(self):
        item = {"label": "findById", "filterText": ".findById", "insertTextFormat": 2,
                "textEdit": {"range": {"start": {"line": 0, "character": 10}, "end": {"line": 0, "character": 12}},
                             "newText": ".findById(${0})"}}
        expected = copy.deepcopy(item)
        expected["filterText"] = "findById"
        response = launch.normalize_completion_filters({"id": 1, "result": {"items": [item]}})
        self.assertEqual(response["result"]["items"], [expected])

    def test_framing_keeps_unicode_and_unrelated_protocol_messages(self):
        completion = {"jsonrpc": "2.0", "id": 4, "result": [{"label": "find", "filterText": ".find", "textEdit": {"newText": ".find()"}}]}
        notification = {"jsonrpc": "2.0", "method": "window/logMessage", "params": {"message": "索引完成"}}
        data = io.BytesIO()
        for message in [completion, notification]:
            body = json.dumps(message, ensure_ascii=False).encode()
            data.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        data.seek(0)
        target = io.BytesIO()
        with patch.object(launch.sys, "stdout", types.SimpleNamespace(buffer=target)):
            launch.forward_server(data)
        target.seek(0)
        messages = []
        while header := target.readline():
            size = int(header.split(b":", 1)[1])
            self.assertEqual(target.readline(), b"\r\n")
            messages.append(json.loads(target.read(size)))
        self.assertEqual(messages[0]["result"][0]["filterText"], "find")
        self.assertEqual(messages[1], notification)

    def test_an_unaccepted_or_changed_eula_never_starts_a_process(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            eula = root / "EULA.txt"
            eula.write_text("Test agreement")
            spec = {"engine": "intellij", "command": ["must-not-start"], "project": tmp,
                    "eula": str(eula), "eulaSha256": hashlib.sha256(eula.read_bytes()).hexdigest(), "eulaAccepted": False}
            path = root / "launch.json"
            with patch.object(launch.subprocess, "Popen") as start:
                for accepted, sha in [(False, spec["eulaSha256"]), (True, "changed")]:
                    spec.update(eulaAccepted=accepted, eulaSha256=sha)
                    path.write_text(json.dumps(spec))
                    with self.assertRaisesRegex(ValueError, "Read and accept"):
                        launch.run(path)
                start.assert_not_called()


if __name__ == "__main__":
    unittest.main()
