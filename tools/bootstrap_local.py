"""Extract only approved apt packages/dependencies locally; no dpkg install or sudo."""
from pathlib import Path
import subprocess, re, json, hashlib
root=Path(__file__).resolve().parents[1]
base=root/'.toolchain'
downloads=base/'downloads'; prefix=base/'root'
downloads.mkdir(parents=True,exist_ok=True);prefix.mkdir(exist_ok=True)
result=subprocess.run(['apt-get','-s','--no-install-recommends','install','rustc','cargo','build-essential','pkg-config'],check=True,capture_output=True,text=True)
packages=re.findall(r'^Inst (\S+) \((\S+)',result.stdout,re.M)
print('approved packages plus dependencies:',len(packages),flush=True)
subprocess.run(['apt-get','download',*[f'{name}={version}' for name,version in packages]],cwd=downloads,check=True)
manifest=[]
for deb in sorted(downloads.glob('*.deb')):
 subprocess.run(['dpkg-deb','-x',str(deb),str(prefix)],check=True)
 manifest.append({'file':deb.name,'sha256':hashlib.sha256(deb.read_bytes()).hexdigest()})
(base/'packages.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Extracted into',prefix,flush=True)
