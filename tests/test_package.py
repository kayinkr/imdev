"""Exercise source packaging without requiring a native build."""
from pathlib import Path
import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
import tarfile
import platform
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]


class SourcePackageTests(unittest.TestCase):
    def test_source_archive_uses_manifest_version(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as directory:
            root = Path(directory)
            (root / 'tools').mkdir()
            shutil.copy2(ROOT / 'tools/package.py', root / 'tools/package.py')
            (root / 'Cargo.toml').write_text('[package]\nname = "imdev"\nversion = "9.8.7"\n')
            subprocess.run([sys.executable, str(root / 'tools/package.py'), '--source-only'],
                           check=True, capture_output=True, text=True)
            manifest = json.loads((root / 'dist/manifest.json').read_text())
            self.assertEqual(manifest[0]['file'], 'Imdev-9.8.7-source.zip')
            with zipfile.ZipFile(root / 'dist' / manifest[0]['file']) as archive:
                self.assertEqual(archive.testzip(), None)
                metadata = tomllib.loads(archive.read('Imdev-9.8.7/Cargo.toml').decode())
                self.assertEqual(metadata['package']['version'], '9.8.7')


    @unittest.skipUnless(platform.system() == 'Linux', 'uses the real Linux Python executable as a packaging fixture')
    def test_native_package_includes_bundled_font_license(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as directory:
            root = Path(directory)
            for folder in ('tools', 'assets', 'target/release'):
                (root / folder).mkdir(parents=True)
            shutil.copy2(ROOT / 'tools/package.py', root / 'tools/package.py')
            # An actual ELF executable tests packaging only, not application behavior.
            shutil.copy2(sys.executable, root / 'target/release/imdev')
            (root / 'Cargo.toml').write_text('[package]\nname = "imdev"\nversion = "9.8.7"\n')
            license_bytes = (ROOT / 'assets/OFL-NotoSansKR.txt').read_bytes()
            (root / 'assets/OFL-NotoSansKR.txt').write_bytes(license_bytes)
            subprocess.run([sys.executable, str(root / 'tools/package.py')],
                           check=True, capture_output=True, text=True)
            archive_path, = (root / 'dist').glob('*.tar.gz')
            with tarfile.open(archive_path) as archive:
                self.assertIn('OFL-NotoSansKR.txt', archive.getnames())
                member = archive.extractfile('OFL-NotoSansKR.txt')
                self.assertIsNotNone(member)
                assert member is not None
                self.assertEqual(member.read(), license_bytes)


if __name__ == '__main__':
    unittest.main()
