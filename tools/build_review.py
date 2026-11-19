"""Create a self-contained review board from actual native software-rendered PNGs."""
from pathlib import Path
import base64
import hashlib
import json
from PIL import Image

root=Path(__file__).resolve().parents[1]
folder=root/'design-preview'
scenes=[('proposal-wide.png','개선 제안 · 넓은 창','1280 × 860 · 로컬 mock 응답 · Body 및 Pretty JSON 실제 헤드리스 포인터 조작'),
        ('proposal-compact.png','개선 제안 · 좁은 창','900 × 720 · 요청/응답 세로 배치 · 영역 내부 스크롤'),
        ('proposal-empty-wide.png','개선 제안 · 초기 상태','1280 × 860 · 응답 대기 상태'),
        ('before-wide.png','이전 0.2.0 · 초기 상태','1280 × 860 · 보존한 변경 전 네이티브 UI')]
records=[]
for file,title,caption in scenes:
    path=folder/file
    with Image.open(path) as image:
        image.verify()
    records.append({'title':title,'caption':caption,'src':'data:image/png;base64,'+base64.b64encode(path.read_bytes()).decode()})
html='''<!doctype html><html lang="ko"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Imdev 0.2.1 UI 개선 제안</title>
<style>html{color-scheme:dark}body{margin:0;padding:24px;background:#14171c;color:#ebeff4;font-family:Arial,'Malgun Gothic',sans-serif;line-height:1.6}header{max-width:1320px}h1{font-size:23px;margin:0 0 8px}p{margin:6px 0;color:#a6b1c0;font-size:14px}.badge{display:inline-block;border:1px solid #725039;color:#ffc08c;padding:3px 9px;font-size:12px;border-radius:4px;margin-bottom:12px}nav{display:flex;gap:8px;flex-wrap:wrap;margin:20px 0 12px}button{padding:9px 14px;background:#272e38;border:1px solid #39414d;border-radius:4px;color:#ebeff4;cursor:pointer}button[aria-pressed=true]{background:#f4994f;color:#1d1915;border-color:#f4994f}button:focus-visible{outline:2px solid white;outline-offset:3px}figure{margin:0;max-width:1280px}img{display:block;width:100%;height:auto;border:1px solid #39414d;border-radius:5px}figcaption{color:#a6b1c0;font-size:13px;margin:10px 0}details{max-width:1280px;margin-top:20px;font-size:14px}li{margin:6px 0}code{overflow-wrap:anywhere}footer{font-size:12px;color:#a6b1c0;margin-top:20px}</style>
<header><span class="badge">제안 A · 사용자 선택 미확정</span><h1>Imdev 0.2.1 · 요청과 응답을 분리한 작업 공간</h1><p>왼쪽 라이브러리, 상단 요청 실행 바, 요청·응답 인스펙터를 독립된 영역으로 정리했습니다.</p><p><strong>검증 범위:</strong> 실제 Rust egui 메시와 폰트 아틀라스를 소프트웨어로 렌더링한 자료입니다. 손으로 다시 그린 HTML 앱이나 실제 Windows OS/GPU 캡처가 아닙니다.</p></header>
<nav aria-label="화면 선택" id="choices"></nav><figure><img id="screen" alt="네이티브 UI 개선 제안"><figcaption id="caption" aria-live="polite"></figcaption></figure>
<details><summary>변경 내용과 검증 한계</summary><ul><li>넓은 창: 요청과 응답을 좌우 배치. 좁은 창: 위아래 배치. 최소 창에서는 전송 바를 유지하고 검사 영역을 스크롤합니다.</li><li>명시적 다크 테마, 대응 텍스트 역할별 Harbor CSS 대비 +1pt, 버튼·탭·간격·정렬 통일.</li><li>주황색 전송 버튼만 주요 동작으로 강조하고 보조 동작은 중립색으로 유지했습니다.</li><li>API 엔진·저장소 코드와 비밀값 저장 방침은 변경하지 않았습니다. 예시 응답은 실제 루프백 mock에서 수신한 시험 데이터입니다.</li><li>Windows 실행 파일은 별도 ZIP으로 제공합니다. 실제 Windows 창·클립보드·한글 IME·GPU 동작은 미검증입니다.</li></ul></details><footer>외부 리소스·추적·브라우저 저장소 없이 로컬 이미지로만 구성한 검토 문서입니다. 개선안을 사용자 확정 요구로 간주하지 않습니다.</footer>
<script>const scenes=SCENES;const nav=document.getElementById('choices');const image=document.getElementById('screen');const caption=document.getElementById('caption');scenes.forEach((scene,i)=>{const b=document.createElement('button');b.type='button';b.textContent=scene.title;b.onclick=()=>select(i);nav.append(b)});function select(i){image.src=scenes[i].src;image.alt=scenes[i].title;caption.textContent=scenes[i].caption;Array.from(nav.children).forEach((b,j)=>b.setAttribute('aria-pressed',String(i===j)))}select(0);</script></html>'''
html=html.replace('SCENES',json.dumps(records,ensure_ascii=False))
out=folder/'Imdev-0.2.1-review.html'
out.write_text(html,encoding='utf-8')
manifest={file:{'sha256':hashlib.sha256((folder/file).read_bytes()).hexdigest(),'size':Image.open(folder/file).size} for file,_,_ in scenes}
(folder/'review-images.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n')
print(out)
print('Screens:',len(records))
