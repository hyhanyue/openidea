"""Release gates: derive the API floor and reject stale bundled components."""
import importlib.util
import pathlib
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location("fork_resources", pathlib.Path(__file__).with_name("prepare-fork-resources.py"))
RESOURCES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RESOURCES)


class ResourceGates(unittest.TestCase):
    def api_fixture(self, root, wit, sdk):
        (root / "wit").mkdir()
        (root / "sdk/rust/openidea-plugin/src").mkdir(parents=True)
        (root / "wit/plugin.wit").write_text(f"package openidea:plugin@{wit}.0;\n")
        (root / "sdk/rust/openidea-plugin/src/lib.rs").write_text(f'pub const API_VERSION: &str = "{sdk}";\n')

    def test_future_api_is_not_replaced_by_the_old_release_floor(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            self.api_fixture(root, "1.4", "1.4")
            with mock.patch.object(RESOURCES, "ROOT", root):
                self.assertEqual(RESOURCES.plugin_api(), "1.4")

    def test_inconsistent_wit_and_sdk_cannot_be_packaged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            self.api_fixture(root, "1.4", "1.3")
            with mock.patch.object(RESOURCES, "ROOT", root):
                with self.assertRaisesRegex(ValueError, "versions must agree"):
                    RESOURCES.plugin_api()

    def test_actual_default_components_match_their_source_fingerprints(self):
        for name in ["mybatis", "spring-beans"]:
            with self.subTest(plugin=name):
                RESOURCES.verify_component(RESOURCES.ROOT / "plugins/official" / name)

    def test_stale_component_is_rejected_before_resource_copy_or_download(self):
        with tempfile.TemporaryDirectory(dir=RESOURCES.ROOT / "target") as directory:
            source = pathlib.Path(directory)
            (source / "dist").mkdir()
            (source / "Cargo.toml").write_text('[package]\nname = "test-plugin"\nversion = "0.1.0"\n')
            (source / "dist/plugin.wasm").write_bytes(b"stale component")
            (source / "dist/plugin.wasm.sources").write_text("stale\n")
            with self.assertRaisesRegex(ValueError, "Stale default plugin"):
                RESOURCES.verify_component(source)


if __name__ == "__main__":
    unittest.main()
