"""Native macOS regression tests for complete app-bundle signatures."""
from pathlib import Path
import os
import platform
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(platform.system() == 'Darwin', 'requires real macOS codesign and ditto')
class MacOSPackageTests(unittest.TestCase):
    def test_extracted_bundle_has_valid_signature_bound_to_plist_and_resources(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as directory:
            root = Path(directory)
            for folder in ('tools', 'target/release'):
                (root / folder).mkdir(parents=True)
            shutil.copy2(ROOT / 'tools/package.py', root / 'tools/package.py')
            (root / 'Cargo.toml').write_text('[package]\nname = "imdev"\nversion = "9.8.7"\n')
            # Compile a real native executable. This tests packaging, not Imdev behavior.
            fixture = root / 'fixture.c'
            fixture.write_text('int main(void) { return 0; }\n')
            subprocess.run(['clang', str(fixture), '-o', str(root / 'target/release/imdev')], check=True)
            subprocess.run([sys.executable, str(root / 'tools/package.py')],
                           check=True, capture_output=True, text=True)
            archive_path, = (root / 'dist').glob('Imdev-*-macOS-*.zip')
            extracted = root / 'extracted'
            subprocess.run(['ditto', '-x', '-k', str(archive_path), str(extracted)], check=True)
            app = extracted / 'Imdev.app'
            result = subprocess.run(['codesign', '--verify', '--deep', '--strict', '--verbose=4', str(app)],
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            with zipfile.ZipFile(archive_path) as archive:
                self.assertIn('Imdev.app/Contents/_CodeSignature/CodeResources', archive.namelist())
            info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
            self.assertEqual(info['CFBundleShortVersionString'], '9.8.7')
            detail = subprocess.run(['codesign', '-d', '--verbose=4', str(app)],
                                    check=True, capture_output=True, text=True)
            self.assertIn('Identifier=com.imdev.httpclient', detail.stderr)
            self.assertNotIn('Info.plist=not bound', detail.stderr)
            self.assertNotIn('Sealed Resources=none', detail.stderr)
            # Verify that the signature actually seals the plist, not just the binary.
            info['CFBundleDisplayName'] = 'Modified after packaging'
            (app / 'Contents/Info.plist').write_bytes(plistlib.dumps(info))
            tampered = subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)],
                                      capture_output=True, text=True)
            self.assertNotEqual(tampered.returncode, 0)


if __name__ == '__main__':
    unittest.main()
