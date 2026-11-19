Imdev 0.2.0 — Windows x64 사용자 테스트 빌드

1. ZIP을 마우스 오른쪽 버튼으로 눌러 '모두 압축 풀기'를 선택하세요.
2. 풀린 Imdev 폴더의 imdev.exe를 더블클릭하세요.
3. Rust, Cargo, Node.js 또는 Python 설치는 필요하지 않습니다.

대상: Windows 10/11 x64 테스트용. 실제 Windows 최소 버전과 GPU/드라이버 호환성은 아직 검증하지 않았습니다. Windows ARM64 네이티브 빌드나 32비트 빌드가 아닙니다.

이 파일은 Linux에서 Windows GNU/MinGW 타깃으로 크로스 빌드한 실제 PE32+ AMD64 GUI 실행 파일입니다. 소스 ZIP이나 Harbor 프로그램이 아닙니다. 기본 다크 테마이며, 대응 텍스트 역할의 글꼴 크기는 기존 CSS 기준 +1pt입니다. 현재 주요 UI 라벨은 영어입니다.

검증 범위:
- Windows release 크로스 컴파일 성공
- PE32+ / AMD64 / Windows GUI 서브시스템 확인
- 정적 DLL import 검사: Windows 시스템 DLL/API-set만 확인
- MinGW libgcc/libstdc++/libwinpthread 또는 Visual C++ 재배포 DLL의 정적 의존성 없음
- ZIP CRC 및 압축 해제한 파일들의 SHA-256 일치 확인
- Linux에서의 기존 자동화/로컬 mock 시험 통과

아직 하지 못한 검증:
- 실제 Windows에서 프로그램 시작/창 표시/그래픽 드라이버
- Windows에서 HTTP/HTTPS 요청, 취소, 저장, 클립보드, 한글 IME
- Windows Defender/SmartScreen 및 조직 보안정책 호환성
- 실제 Windows에서 모든 시스템 DLL/API-set과 동적 그래픽 라이브러리 로딩

서명되지 않은 테스트 빌드입니다. SmartScreen/보안 경고가 표시될 수 있습니다. 파일 출처와 해시를 확인하고 조직 정책에 따라 실행하세요. 보안 기능을 끄거나 인터넷의 임의 DLL을 내려받지 마세요. 오류가 나면 경고·오류 문구와 Windows 버전을 전달해 주세요. URL, 토큰, 응답 본문 등 비밀정보는 지워서 보내세요.

실제 API 자동 호출은 하지 않습니다. 개발팀 시험은 로컬 mock 서버만 사용했습니다. 처음에는 실제 비밀번호나 토큰 대신 시험 데이터를 사용하세요.

저장 정책:
- 원본 요청/응답과 비밀값은 자동 저장하지 않습니다.
- 저장 템플릿은 URL의 origin만 남기고 경로/값/본문을 비웁니다. 정확한 재실행용 저장이 아닙니다.
- 이름과 키 이름은 평문 저장되므로 비밀정보를 넣지 마세요.
- 기본 저장 위치: %LOCALAPPDATA%\Imdev\state.json
- 같은 저장 폴더를 공유하는 Imdev는 한 개만 실행하세요.

그래픽 드라이버의 OpenGL 지원이 필요합니다. 그래픽 DLL이나 드라이버는 이 ZIP에 포함하지 않습니다.

licenses 폴더에는 포함된 글꼴과 제3자 구성요소의 라이선스 고지가 들어 있습니다. 이 폴더도 함께 보관하세요.
