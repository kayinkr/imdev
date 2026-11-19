Imdev 0.3.0 — Windows x64 요청 복원·공통 인증 테스트 빌드
제작 프로파일: 나락

실행
1. ZIP을 모두 압축 해제한 뒤 Imdev\imdev.exe를 실행하세요.
2. Rust/Cargo/Node.js/Python 설치는 필요하지 않습니다.
3. Windows 10/11 x64 테스트용 크로스 빌드입니다. 실제 Windows 실행은 아직 검증하지 못했습니다. 서명되지 않았으며 OpenGL 그래픽 지원이 필요합니다. 조직 정책을 따르고 보안 기능을 끄거나 임의 DLL을 다운로드하지 마세요.

이번 반영
- 기본 다크 모드와 Harbor 기준 역할별 +1pt 글꼴 유지.
- 요청/응답 분리 디자인 제안, History/Clear history 및 Timeout 행 정렬.
- 일반 요청의 URL 경로·중복 query·메서드·Headers·Body·형식·Timeout을 암호화 저장하여 History와 저장 템플릿에서 편집기로 복원.
- History/템플릿 선택은 전송하지 않습니다. 수정 중인 내용을 교체할 때 확인하며, Send를 눌러야 전송합니다.
- 공통 인증에서 HTTP(S) origin(스킴/호스트/포트)과 Bearer 토큰을 설정합니다. 토큰 입력에는 Bearer 접두어를 넣지 마세요.
- Auto는 지정 origin과 일치할 때만 공통 인증을 적용합니다. Common은 일치하지 않으면 오류, None은 자동 인증 없이 보냅니다. 토큰 발급 요청에는 None을 선택할 수 있습니다.
- 공통 인증이 적용될 때 수동 Authorization이 있으면 충돌로 차단하며 중복 Authorization도 차단합니다. 최신 토큰은 Send 시점에 적용합니다. 토큰 발급·재발급 자동화는 없습니다.

저장과 보안
- 새 파일: %LOCALAPPDATA%\Imdev\state.requests-v2.enc
- Windows 현재 사용자 DPAPI로 일반 요청 데이터와 메타데이터를 암호화합니다. 인증 토큰은 세션 메모리에만 있으며 재시작 후 재입력이 필요합니다. 저장된 요청에는 공통 인증 참조와 origin 설정만 남습니다.
- 알려진 Authorization/토큰/client_secret/cookie/암호 계열 값은 암호화된 내용에서도 제외합니다. Headers/Query/JSON 항목, URL 경로 구간 및 전체 Body를 사용자 저장 제외로 추가 지정할 수 있습니다.
- 임의 비밀값을 완벽히 탐지하지는 못합니다. 일반 필드, URL 경로, JSON 키, 템플릿 이름에도 비밀을 넣지 말고 저장 제외를 명시하세요. 알 수 없는 자유 형식 본문은 전체 Body 제외를 사용하세요.
- 제외한 값은 재입력 표식으로 복원됩니다. 경고를 검토하고 실제 값을 입력하거나 항목을 제거해야 Send할 수 있습니다.
- 응답 본문/헤더 및 상세 오류 내용은 저장하지 않습니다. 명시적으로 Copy를 누른 내용은 시스템 클립보드로 전달됩니다.
- 암호화·복호화·쓰기 실패 시 평문 저장이나 기존 파일 초기화를 하지 않습니다. 실패 경고를 확인하세요. 같은 저장 폴더를 공유하는 앱은 하나만 실행하세요.
- 이전 state.json은 읽기 전용으로 보존합니다. 이전 origin-only 기록에서 이미 버려진 path/query/header/body는 복구할 수 없습니다. Legacy 부분 불러오기는 완전한 재사용이 아니며 기본 인증 None으로 열립니다. Clear history는 새 암호화 기록만 지웁니다.
- DPAPI는 Windows 사용자/보호 환경에 묶입니다. 다른 PC나 사용자로 파일만 복사했을 때 복호화를 보장하지 않습니다. 원본 파일을 보존하세요.
- 자동 재시도/redirect 및 TLS 검증 해제를 하지 않으며 원격 수집은 없습니다.

검증 범위
- Linux에서 전체 Rust 테스트 77개 통과. 로컬 mock과 합성 데이터만 사용했습니다.
- 암호화 저장/재열기는 테스트 전용 실제 AES-256-GCM 주입 백엔드로 실행했습니다. 이는 Windows DPAPI 실행 검증이 아닙니다.
- Windows DPAPI 코드는 Windows 테스트 실행 파일까지 크로스 컴파일했지만 Windows에서 실행하지 않았습니다.
- 실제 Windows 창/GPU/한글 입력/클립보드/네트워크/DPAPI는 미검증입니다. macOS M1 빌드도 이 ZIP에 포함되지 않습니다.
- 화면 미리보기는 실제 egui 메시를 소프트웨어 렌더링한 것으로 Windows OS 캡처가 아닙니다.

오류 제보 시 Windows 버전과 경고 문구, 창 크기/문제 위치를 알려주세요. URL·토큰·요청/응답의 민감정보를 제거하세요. 기존 배포본은 별도 보존되어 있습니다.
licenses 폴더와 BUILD-VERIFICATION.json을 실행 파일과 함께 보관하세요.
