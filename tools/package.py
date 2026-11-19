"""Package only a binary built for the current host. Does not cross-compile or sign."""
import argparse
from pathlib import Path
import platform
import shutil
import stat
import tarfile
import tempfile
import zipfile
import json
import hashlib
import plistlib
import tomllib

root = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--source-only', action='store_true')
args = p.parse_args()
version = tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
dist = root / 'dist'
dist.mkdir(exist_ok=True)

def source_files():
    for name in ('Cargo.toml', 'Cargo.lock', 'README.md', 'VERIFICATION.md', 'PREPARATION.md', 'LICENSE'):
        path = root / name
        if path.is_file():
            yield path
    for folder in ('src', 'tests', 'tools', 'docs', 'assets'):
        for path in (root / folder).rglob('*'):
            if path.is_file() and '__pycache__' not in path.parts:
                yield path

source = dist / f'Imdev-{version}-source.zip'
with zipfile.ZipFile(source, 'w', zipfile.ZIP_DEFLATED) as z:
    for path in sorted(source_files()):
        z.write(path, Path(f'Imdev-{version}') / path.relative_to(root))
with zipfile.ZipFile(source) as z:
    assert z.testzip() is None
outputs = [source]
if not args.source_only:
    system = platform.system()
    arch = platform.machine().lower()
    binary = root / 'target/release' / ('imdev.exe' if system == 'Windows' else 'imdev')
    if not binary.is_file():
        raise SystemExit('Build the native release binary first: cargo build --release --locked')
    header = binary.read_bytes()[:4]
    expected = {'Linux': b'\x7fELF', 'Windows': b'MZ', 'Darwin': b'\xcf\xfa\xed\xfe'}
    if system not in expected or not header.startswith(expected[system]):
        raise SystemExit('Binary format does not match the packaging host')
    with tempfile.TemporaryDirectory(prefix='imdev-package-', dir=dist) as temp:
        temp = Path(temp)
        if system == 'Darwin':
            bundle = temp / 'Imdev.app'
            macos = bundle / 'Contents/MacOS'
            macos.mkdir(parents=True)
            shutil.copy2(binary, macos / 'imdev')
            (macos / 'imdev').chmod(0o755)
            info = {'CFBundleName': 'Imdev', 'CFBundleDisplayName': 'Imdev', 'CFBundleExecutable': 'imdev', 'CFBundleIdentifier': 'com.imdev.httpclient', 'CFBundlePackageType': 'APPL', 'CFBundleShortVersionString': version, 'CFBundleVersion': version, 'NSHighResolutionCapable': True}
            (bundle / 'Contents/Info.plist').write_bytes(plistlib.dumps(info))
        else:
            bundle = temp / 'Imdev'
            bundle.mkdir()
            shutil.copy2(binary, bundle / binary.name)
        for name in ('README.md', 'VERIFICATION.md', 'LICENSE'):
            if (root / name).is_file():
                shutil.copy2(root / name, temp / name)
        font_license = root / 'assets/OFL-NotoSansKR.txt'
        if font_license.is_file():
            shutil.copy2(font_license, temp / font_license.name)
        label = {'Darwin': 'macOS', 'Windows': 'Windows', 'Linux': 'Linux'}[system]
        artifact = dist / f'Imdev-{version}-{label}-{arch}.{ "tar.gz" if system == "Linux" else "zip"}'
        if system == 'Linux':
            with tarfile.open(artifact, 'w:gz') as tar:
                for path in temp.iterdir():
                    tar.add(path, arcname=path.name)
        else:
            with zipfile.ZipFile(artifact, 'w', zipfile.ZIP_DEFLATED) as z:
                for path in sorted(temp.rglob('*')):
                    if path.is_file():
                        z.write(path, path.relative_to(temp))
            with zipfile.ZipFile(artifact) as z:
                assert z.testzip() is None
        outputs.append(artifact)
manifest = [{'file': path.name, 'bytes': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for path in outputs]
(dist / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest, indent=2))
