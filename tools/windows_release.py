"""Inspect and package the actual Imdev Windows GNU release; never substitute sources."""
import struct
from pathlib import Path
import hashlib
import json
import re
import shutil
import subprocess
import tempfile
import tomllib
import zipfile


def verify_archive(path, expected):
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)) or set(names) != set(expected):
            raise ValueError('Archive entries differ from staged payload')
        for name in names:
            parts = Path(name).parts
            if name.startswith(('/', '\\')) or '..' in parts or '\\' in name:
                raise ValueError('Unsafe ZIP member path')
        if archive.testzip() is not None:
            raise ValueError('ZIP CRC failure')
        with tempfile.TemporaryDirectory(prefix='imdev-zip-check-') as tmp:
            archive.extractall(tmp)
            for name in names:
                digest = hashlib.sha256((Path(tmp) / name).read_bytes()).hexdigest()
                if digest != expected[name]:
                    raise ValueError('Extracted payload hash mismatch: ' + name)
        return sorted(names)


def classify_imports(imports):
    known = {'advapi32.dll', 'bcrypt.dll', 'bcryptprimitives.dll', 'crypt32.dll', 'dwmapi.dll',
             'gdi32.dll', 'imm32.dll', 'kernel32.dll', 'msvcrt.dll', 'ntdll.dll',
             'ole32.dll', 'opengl32.dll', 'shell32.dll', 'shlwapi.dll',
             'user32.dll', 'userenv.dll', 'uxtheme.dll', 'ws2_32.dll'}
    names = sorted({name.lower() for name in imports})
    system = [name for name in names if name in known or
              (name.startswith('api-ms-win-') and name.endswith('.dll') and '/' not in name and '\\' not in name)]
    return system, sorted(set(names) - set(system))


def pe_header(data):
    if len(data) < 64 or data[:2] != b'MZ':
        raise ValueError('Not a DOS/PE executable')
    offset = struct.unpack_from('<I', data, 0x3c)[0]
    optional = offset + 24
    if optional + 70 > len(data) or data[offset:offset + 4] != b'PE\0\0':
        raise ValueError('Invalid or truncated PE headers')
    return {'machine': struct.unpack_from('<H', data, offset + 4)[0],
            'optional_magic': struct.unpack_from('<H', data, optional)[0],
            'subsystem': struct.unpack_from('<H', data, optional + 68)[0]}


def write_payload(archive_path, stage):
    expected = {}
    with zipfile.ZipFile(archive_path, 'x', zipfile.ZIP_DEFLATED, compresslevel=9, strict_timestamps=False) as archive:
        for file in sorted(stage.rglob('*')):
            if file.is_file():
                name = file.relative_to(stage.parent).as_posix()
                if name.lower() in {n.lower() for n in expected}:
                    raise ValueError('Case-insensitive ZIP name collision')
                expected[name] = hashlib.sha256(file.read_bytes()).hexdigest()
                archive.write(file, name)
    return expected


def create_release(root, tag='test'):
    if not re.fullmatch(r'[a-z0-9-]+', tag):
        raise ValueError('Invalid release tag')
    version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
    binary = root / 'target/windows-cross/x86_64-pc-windows-gnu/release/imdev.exe'
    header = pe_header(binary.read_bytes())
    if header != {'machine': 0x8664, 'optional_magic': 0x20b, 'subsystem': 2}:
        raise ValueError('Expected actual AMD64 PE32+ Windows GUI executable')
    objdump = root / '.toolchain/windows/mingw/usr/bin/x86_64-w64-mingw32-objdump'
    inspection = subprocess.check_output([str(objdump), '-p', str(binary)], text=True)
    imports = re.findall(r'DLL Name:\s*(\S+)', inspection)
    if not imports:
        raise ValueError('Import table inspection returned no DLLs')
    system, external = classify_imports(imports)
    if external:
        raise ValueError('Unresolved non-system DLLs; do not ship incomplete ZIP: ' + ', '.join(external))
    output = root / f'releases/windows-x64-{version}-{tag}'
    output.mkdir(parents=True, exist_ok=False)
    stage = output / 'staging/Imdev'
    stage.mkdir(parents=True)
    shutil.copy2(binary, stage / 'imdev.exe')
    readme = root / f'docs/README-Windows-{version}.ko.txt'
    if not readme.is_file():
        readme = root / 'docs/README-Windows.ko.txt'
    shutil.copy2(readme, stage / 'README-Windows.ko.txt')
    licenses = stage / 'licenses'
    licenses.mkdir()
    shutil.copy2(root / 'assets/OFL-NotoSansKR.txt', licenses / 'OFL-NotoSansKR.txt')
    (licenses / 'LICENSE-Imdev.txt').write_text('''MIT License

Copyright (c) Imdev contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
''', encoding='utf-8')
    tree = subprocess.check_output(['sh', str(root / 'tools/cargo-windows.sh'), 'tree', '--locked', '--offline', '--target', 'x86_64-pc-windows-gnu', '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}'], cwd=root, text=True)
    crates = sorted(set(re.findall(r'^([\w-]+) v([^\s]+)', tree, re.M)) - {('imdev', version)})
    registry = root / '.toolchain/cargo-home/registry/src'
    license_records = []
    for name, crate_version in crates:
        locations = list(registry.glob(f'*/{name}-{crate_version}'))
        if len(locations) != 1:
            raise ValueError('Cannot identify dependency license source: ' + name)
        crate = locations[0]
        meta = tomllib.loads((crate / 'Cargo.toml').read_text())['package']
        copied = []
        for file in sorted(crate.rglob('*')):
            upper = file.name.upper()
            if file.is_file() and not file.is_symlink() and any(word in upper for word in ('LICEN', 'COPYING', 'NOTICE', 'COPYRIGHT', 'UNLICENSE')) and file.stat().st_size <= 2 * 1024 * 1024:
                dest = licenses / 'crates' / f'{name}-{crate_version}' / file.relative_to(crate)
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(file, dest)
                copied.append(str(file.relative_to(crate)))
        license_records.append({'crate': name, 'version': crate_version, 'license': meta.get('license'), 'license_file': meta.get('license-file'), 'copied_notices': copied})
    rust_docs = root / '.toolchain/windows/rustup/toolchains/1.93.1-x86_64-unknown-linux-gnu/share/doc/rust'
    for file in sorted(rust_docs.rglob('*')):
        if file.is_file() and (file.name.startswith('COPYRIGHT') or 'licenses' in file.relative_to(rust_docs).parts):
            dest = licenses / 'rust-runtime' / file.relative_to(rust_docs)
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(file, dest)
    mingw_docs = root / '.toolchain/windows/mingw/usr/share/doc'
    for file in sorted(mingw_docs.rglob('copyright')):
        dest = licenses / 'mingw' / file.relative_to(mingw_docs)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(file, dest)
    (licenses / 'dependency-index.json').write_text(json.dumps(license_records, indent=2) + '\n')
    report = {'version': version, 'target': 'x86_64-pc-windows-gnu', 'pe_header': header,
              'static_imports': system, 'non_system_imports': external, 'bundled_dlls': [],
              'exe_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'windows_execution_verified': False, 'code_signed': False,
              'scope': 'Cross-compiled and statically inspected on Linux; Windows GUI, system API resolution, dynamic GPU loading and runtime behavior are unverified.',
              'rebuild': 'sh tools/cargo-windows.sh rustc --release --locked --target x86_64-pc-windows-gnu --bin imdev -- -C link-arg=-Wl,--subsystem,windows'}
    (stage / 'BUILD-VERIFICATION.json').write_text(json.dumps(report, indent=2) + '\n')
    archive_path = output / f'Imdev-{version}-Windows-x64-test.zip'
    expected = write_payload(archive_path, stage)
    names = verify_archive(archive_path, expected)
    archive_sha = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    report.update(zip_path=str(archive_path), zip_sha256=archive_sha, zip_bytes=archive_path.stat().st_size,
                  zip_files=len(names), extraction_hashes_verified=True, zip_crc_verified=True,
                  dependency_notice_packages=len(license_records))
    (output / 'verification.json').write_text(json.dumps(report, indent=2) + '\n')
    (output / 'payload-sha256.json').write_text(json.dumps(expected, indent=2) + '\n')
    (output / 'imdev-objdump.txt').write_text(inspection)
    (output / 'SHA256SUMS.txt').write_text(f'{archive_sha}  {archive_path.name}\n{report["exe_sha256"]}  imdev.exe\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', default='test')
    args = parser.parse_args()
    create_release(Path(__file__).resolve().parents[1], args.tag)
