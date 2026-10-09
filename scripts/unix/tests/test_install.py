#!/usr/bin/env python3
"""Verify the Unix installer against local release fixtures."""

import json
import os
from pathlib import Path
import platform
import subprocess
import tarfile
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().parents[3] / "install.sh"
SYSTEM = {"Darwin": "macos", "Linux": "linux"}.get(platform.system())
ARCH = {"x86_64": "x86_64", "amd64": "x86_64", "arm64": "arm64", "aarch64": "arm64"}.get(platform.machine())


def run_installer(root, release, archive, destination):
    """Run install.sh with a fake curl serving `release` (API JSON) and `archive`."""
    release_json = root / "release.json"
    release_json.write_text(json.dumps(release))
    tools = root / "tools"
    tools.mkdir()
    curl = tools / "curl"
    curl.write_text("""#!/bin/sh
set -eu
if [ "$#" -eq 4 ] && [ "$2" = "-o" ]; then
    cp "$MCT_TEST_ARCHIVE" "$3"
else
    cat "$MCT_TEST_RELEASE_JSON"
fi
""")
    curl.chmod(0o755)
    env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ["PATH"], INSTALL_DIR=str(destination), MCT_TEST_ARCHIVE=str(archive), MCT_TEST_RELEASE_JSON=str(release_json))
    return subprocess.run(["bash"], input=INSTALLER.read_text(), env=env, capture_output=True, text=True, timeout=30)


@unittest.skipUnless(SYSTEM and ARCH, "the Unix installer does not support this platform")
class InstallerTest(unittest.TestCase):
    def test_pipeline_installs_both_binaries_into_the_requested_directory(self):
        with tempfile.TemporaryDirectory(prefix="mct-installer-test-") as temporary:
            root = Path(temporary)
            # Same layout as release.yml packages: one top-level directory.
            package = root / "mini-consumes-tokens-v-test"
            package.mkdir()
            for name in ("mct-cli", "mct-mcp-server"):
                (package / name).write_text("#!/bin/sh\nexit 0\n")
            archive = root / "release.tar.gz"
            with tarfile.open(archive, "w:gz") as tar:
                tar.add(package, arcname=package.name)
            release = {"tag_name": "v-test", "assets": [{"browser_download_url": f"https://fixture.invalid/mini-consumes-tokens-v-test-{SYSTEM}-{ARCH}.tar.gz"}]}
            destination = root / "custom destination"
            result = run_installer(root, release, archive, destination)
            self.assertEqual(result.returncode, 0, result.stderr)
            for name in ("mct-cli", "mct-mcp-server"):
                installed = destination / name
                self.assertEqual(installed.read_bytes(), (package / name).read_bytes())
                self.assertTrue(os.access(installed, os.X_OK))

    def test_missing_release_fails_with_an_explanation(self):
        # What the releases/latest API returns while no release is published.
        with tempfile.TemporaryDirectory(prefix="mct-installer-test-") as temporary:
            root = Path(temporary)
            destination = root / "destination"
            result = run_installer(root, {"message": "Not Found", "status": "404"}, root / "unused", destination)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("could not determine the latest release tag", result.stderr)
            self.assertFalse(destination.exists())


if __name__ == "__main__":
    unittest.main()
