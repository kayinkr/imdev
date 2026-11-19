import importlib.util
from pathlib import Path
import struct
import unittest
import tempfile
import zipfile
import hashlib
import os

MODULE_PATH = Path(__file__).resolve().parents[1] / 'tools/windows_release.py'
spec = importlib.util.spec_from_file_location('windows_release', MODULE_PATH)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class PEHeaderTests(unittest.TestCase):
    def test_packager_preserves_ancient_license_file_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            stage = Path(tmp) / 'Imdev'
            stage.mkdir()
            payload = stage / 'LICENSE.txt'
            payload.write_bytes(b'License fixture with archive epoch timestamp')
            os.utime(payload, (1, 1))
            archive = Path(tmp) / 'release.zip'
            expected = module.write_payload(archive, stage)
            self.assertEqual(module.verify_archive(archive, expected), ['Imdev/LICENSE.txt'])
            with zipfile.ZipFile(archive) as z:
                self.assertEqual(z.read('Imdev/LICENSE.txt'), payload.read_bytes())
                self.assertEqual(z.getinfo('Imdev/LICENSE.txt').date_time[:3], (1980, 1, 1))
    def test_archive_verification_detects_mismatched_payload(self):
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / 'test.zip'
            with zipfile.ZipFile(archive, 'w') as z:
                z.writestr('Imdev/imdev.exe', b'synthetic test payload')
            expected = {'Imdev/imdev.exe': hashlib.sha256(b'synthetic test payload').hexdigest()}
            self.assertEqual(module.verify_archive(archive, expected), ['Imdev/imdev.exe'])
            with self.assertRaises(ValueError):
                module.verify_archive(archive, {'Imdev/imdev.exe': 'incorrect'})
    def test_import_classification_is_case_insensitive_and_rejects_external_runtimes(self):
        system, external = module.classify_imports(['KERNEL32.dll', 'kernel32.dll', 'api-ms-win-core-path-l1-1-0.dll', 'libwinpthread-1.dll'])
        self.assertEqual(system, ['api-ms-win-core-path-l1-1-0.dll', 'kernel32.dll'])
        self.assertEqual(external, ['libwinpthread-1.dll'])
    def test_dpapi_crypt32_is_system_but_similarly_named_dll_is_not(self):
        system, external = module.classify_imports(['CRYPT32.dll', 'crypt32.dll', 'crypt32-helper.dll'])
        self.assertEqual(system, ['crypt32.dll'])
        self.assertEqual(external, ['crypt32-helper.dll'])
    def test_reads_amd64_pe32_plus_gui_header(self):
        fixture = bytearray(512)
        fixture[:2] = b'MZ'
        struct.pack_into('<I', fixture, 0x3c, 0x80)
        fixture[0x80:0x84] = b'PE\0\0'
        struct.pack_into('<H', fixture, 0x84, 0x8664)
        struct.pack_into('<H', fixture, 0x94, 0xf0)
        struct.pack_into('<H', fixture, 0x98, 0x20b)
        struct.pack_into('<H', fixture, 0x98 + 68, 2)
        self.assertEqual(module.pe_header(bytes(fixture)), {'machine': 0x8664, 'optional_magic': 0x20b, 'subsystem': 2})
    def test_rejects_non_pe_and_truncated_files(self):
        for data in (b'', b'\x7fELF' + bytes(500), b'MZ' + bytes(10)):
            with self.assertRaises(ValueError):
                module.pe_header(data)

if __name__ == '__main__':
    unittest.main()
