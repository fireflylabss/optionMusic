#!/usr/bin/env python3
"""Exercise release metadata refresh with local archives and real makepkg."""

import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest


AUR_ROOT = Path(__file__).resolve().parents[1]
PACKAGES = ("stillmusic", "stillmusic-bin")


@unittest.skipUnless(shutil.which("makepkg"), "makepkg is required")
class BumpTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="still-aur-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.aur = self.root / "aur"
        self.aur.mkdir()
        shutil.copy2(AUR_ROOT / "bump.sh", self.aur / "bump.sh")
        for package in PACKAGES:
            shutil.copytree(AUR_ROOT / package, self.aur / package)
        self.fixtures = self.root / "fixtures"
        self.fixtures.mkdir()
        mock_bin = self.root / "bin"
        mock_bin.mkdir()
        curl = mock_bin / "curl"
        curl.write_text("""#!/usr/bin/env bash
set -euo pipefail
for ((i=1;i<=$#;i++)); do
  if [[ "${!i}" == --output ]]; then
    ((i+=1))
    output="${!i}"
  fi
done
url="${!#}"
printf '%s\\n' "$url" >> "$CURL_LOG"
if [[ "$url" == */archive/* ]]; then
  cp "$FIXTURE_DIR/source.tar.gz" "$output"
else
  [[ "${FAIL_BINARY:-0}" == 0 ]] || exit 22
  cp "$FIXTURE_DIR/binary.tar.gz" "$output"
fi
""")
        curl.chmod(0o755)
        self.env = dict(os.environ, PATH=f"{mock_bin}:{os.environ['PATH']}",
                        FIXTURE_DIR=str(self.fixtures),
                        CURL_LOG=str(self.root / "curl.log"))

    def archive(self, name, files):
        archive_path = self.fixtures / f"{name}.tar.gz"
        with tarfile.open(archive_path, "w:gz") as archive:
            for filename in files:
                data = b"fixture\n"
                info = tarfile.TarInfo(filename)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
        return hashlib.sha256(archive_path.read_bytes()).hexdigest()

    def fixtures_for(self, tag):
        return (
            self.archive("source", [f"optionMusic-{tag.removeprefix('v')}/Cargo.toml"]),
            self.archive("binary", ["still", "msc", "LICENSE", "README.md"]),
        )

    def bump(self, tag):
        return subprocess.run([str(self.aur / "bump.sh"), tag], env=self.env,
                              text=True, capture_output=True)

    def test_version_mapping_checksums_and_metadata(self):
        for tag, version in (("v0.3.0-beta", "0.3.0beta"),
                             ("v0.3.0-beta.1", "0.3.0beta1"),
                             ("v0.3.0-beta.2", "0.3.0beta2"),
                             ("v0.3.0-alpha.1", "0.3.0alpha1"),
                             ("v0.3.0-stable", "0.3.0")):
            with self.subTest(tag=tag):
                checksums = self.fixtures_for(tag)
                result = self.bump(tag)
                self.assertEqual(result.returncode, 0, result.stderr)
                for package, checksum in zip(PACKAGES, checksums):
                    recipe = (self.aur / package / "PKGBUILD").read_text()
                    metadata = (self.aur / package / ".SRCINFO").read_text()
                    self.assertIn(f"pkgver={version}\n", recipe)
                    self.assertIn(f"_tag={tag[1:]}\n", recipe)
                    self.assertIn(f"pkgver = {version}\n", metadata)
                    self.assertIn(checksum, recipe)
                    self.assertIn(checksum, metadata)

    def test_failed_binary_download_preserves_both_recipes(self):
        self.fixtures_for("v0.3.0-beta.1")
        before = {path: path.read_bytes() for package in PACKAGES
                  for path in (self.aur / package).iterdir() if path.is_file()}
        self.env["FAIL_BINARY"] = "1"
        self.assertNotEqual(self.bump("v0.3.0-beta.1").returncode, 0)
        for path, original in before.items():
            self.assertEqual(path.read_bytes(), original, str(path))

    def test_invalid_tag_rejected_before_download(self):
        for tag in ["v0.3.0-beta/unsafe", "v0.3.0-beta.0", "v0.3.0-beta.01"]:
            with self.subTest(tag=tag):
                self.assertNotEqual(self.bump(tag).returncode, 0)
                self.assertFalse((self.root / "curl.log").exists())


if __name__ == "__main__":
    unittest.main()
