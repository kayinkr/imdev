"""Install approved Windows cross tools under this project only; no sudo/profile edits."""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess
import urllib.request

root = Path(__file__).resolve().parents[1]
base = root / '.toolchain' / 'windows'
downloads = base / 'downloads'
prefix = base / 'mingw'
downloads.mkdir(parents=True, exist_ok=True)
prefix.mkdir(exist_ok=True)
plan = subprocess.run(['apt-get', '-s', '--no-install-recommends', 'install', 'gcc-mingw-w64-x86-64-posix'], check=True, capture_output=True, text=True).stdout
packages = re.findall(r'^Inst (\S+) \((\S+)', plan, re.M)
allowed = {'gcc-mingw-w64-x86-64-posix', 'gcc-mingw-w64-x86-64-posix-runtime', 'gcc-mingw-w64-base', 'binutils-mingw-w64-base', 'binutils-mingw-w64-x86-64', 'mingw-w64-common', 'mingw-w64-x86-64-dev'}
if not packages or set(name for name, version in packages) - allowed:
    raise SystemExit('Dependency plan changed; request approval before proceeding')
print('Extracting approved MinGW packages:', [name for name, version in packages], flush=True)
subprocess.run(['apt-get', 'download', *[f'{name}={version}' for name, version in packages]], cwd=downloads, check=True)
manifest = []
for deb in sorted(downloads.glob('*.deb')):
    subprocess.run(['dpkg-deb', '-x', str(deb), str(prefix)], check=True)
    manifest.append({'file': deb.name, 'sha256': hashlib.sha256(deb.read_bytes()).hexdigest()})
url = 'https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init'
installer = downloads / 'rustup-init'
expected = urllib.request.urlopen(url + '.sha256', timeout=60).read().decode().split()[0]
if not installer.exists() or hashlib.sha256(installer.read_bytes()).hexdigest() != expected:
    with urllib.request.urlopen(url, timeout=120) as response, installer.open('wb') as output:
        while chunk := response.read(1024 * 1024):
            output.write(chunk)
actual = hashlib.sha256(installer.read_bytes()).hexdigest()
if actual != expected:
    raise SystemExit('Official rustup installer checksum mismatch')
installer.chmod(0o755)
env = os.environ.copy()
env.update(RUSTUP_HOME=str(base / 'rustup'), CARGO_HOME=str(base / 'cargo'), RUSTUP_INIT_SKIP_PATH_CHECK='yes')
subprocess.run([str(installer), '-y', '--no-modify-path', '--profile', 'minimal', '--default-host', 'x86_64-unknown-linux-gnu', '--default-toolchain', '1.93.1'], env=env, check=True)
subprocess.run([str(base / 'cargo/bin/rustup'), 'target', 'add', '--toolchain', '1.93.1', 'x86_64-pc-windows-gnu'], env=env, check=True)
manifest.append({'file': 'rustup-init', 'url': url, 'sha256': actual})
(base / 'install-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print('Isolated toolchains ready:', base, flush=True)
