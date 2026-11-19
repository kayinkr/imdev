Imdev 0.3.1 — Windows x64 Key/Value 표 입력 테스트 빌드
제작 프로파일: 나락

실행: ZIP 전체를 압축 해제하고 Imdev\imdev.exe를 실행하세요. Rust/Cargo/Node/Python 별도 설치는 필요하지 않습니다. Windows 10/11 x64 테스트용 미서명 크로스 빌드이며 OpenGL 그래픽 지원이 필요합니다. 실제 Windows 실행은 아직 미검증입니다. 조직 정책을 따르고 보안 기능을 끄거나 임의 DLL을 다운로드하지 마세요.

0.3.1 변경
- Query/Headers를 Key와 Value 분리 표로 입력합니다. Add row, Duplicate, Delete를 제공합니다.
- 중복 키와 입력 순서, 빈 값, 값의 콜론 및 공백을 보존합니다. 완전히 빈 행은 제외하고 키 없이 값만 있는 행은 Save/Send 시 오류로 안내합니다.
- URL 자체의 query와 별도 Query 표는 구분합니다. 복원 시 URL query를 표에 다시 넣지 않으므로 이중 추가하지 않습니다.
- 기존 History/template tuple 데이터와 암호화 저장 형식은 유지합니다.
- 알려진 민감값 및 사용자 제외 값은 표에서 마스킹합니다. 제외된 값은 재입력 표식과 안내가 나타나며, 실제 입력하거나 삭제해야 Send할 수 있습니다. 표시/숨기기 토글은 이번 버전에 없습니다.
- Body 형식/편집 방식, 다크 기본/+1pt 및 행 높이 정렬은 유지합니다.

유지된 저장·인증 정책
- 일반 요청의 전체 URL, Query, Headers, Body/형식, Timeout과 인증 참조를 Windows 현재 사용자 DPAPI로 암호화 저장합니다.
- 새 저장 위치: %LOCALAPPDATA%\Imdev\state.requests-v2.enc. IMDEV_DATA_DIR override를 유지합니다.
- 이전 state.json은 읽기 전용으로 보존합니다. 과거 origin-only 기록에서 버려진 path/query/header/body는 복구할 수 없습니다. Legacy 부분 불러오기는 완전한 재사용이 아니며 기본 인증 None입니다. Clear history는 새 기록만 지웁니다.
- 공통 Bearer 토큰은 세션 메모리에만 보관합니다. 재시작 후 재입력해야 하며 Send 시 최신 토큰을 적용합니다. 공통 설정에는 origin과 Bearer 접두어 없는 토큰을 입력하세요.
- Auto는 정확히 일치하는 scheme/host/port origin에만 적용합니다. Common은 origin 불일치를 오류로 차단합니다. None은 공통 인증을 자동 적용하지 않아 토큰 발급 요청에 사용할 수 있습니다.
- 공통 인증과 수동 Authorization 충돌, 중복 Authorization은 차단합니다. 토큰 발급·갱신 자동화는 없습니다.
- History/템플릿 선택은 편집기로 불러올 뿐 전송하지 않습니다. 수정된 내용을 교체할 때 확인하고 Send를 눌러야 전송합니다.
- Authorization/알려진 token/client_secret/cookie/암호 항목은 암호화된 내용에서도 제외합니다. 임의 비밀값 완전 탐지는 보장하지 않습니다. Query/Header 키, JSON pointer, URL 경로 구간, 전체 Body 제외를 명시적으로 지정하세요. 템플릿 이름·일반 키·URL 경로에도 비밀을 넣지 마세요.
- 응답 본문/헤더와 상세 오류는 저장하지 않습니다. 암호화/복호화/쓰기 실패 시 평문 우회나 기존 파일 초기화를 하지 않습니다. DPAPI 파일만 다른 사용자/PC에 복사했을 때 복호화는 보장하지 않습니다.
- 같은 저장 폴더를 공유하는 앱은 하나만 실행하세요. 자동 재시도/redirect/TLS 검증 해제/원격 수집을 추가하지 않았습니다.

검증과 한계
- Linux에서 전체 Rust 테스트 87개 통과. 합성 데이터와 localhost mock만 사용했습니다. URL query와 추가 행이 저장소 재열기 전후 한 번씩만 전송되는 회귀를 포함합니다.
- 실제 표 입력/추가/복제/삭제 pointer·keyboard 이벤트, 민감값 마스킹, 기존 인증/복원/암호화/Send 경계 회귀를 시험했습니다.
- Windows 실행 파일은 실제 PE32+ AMD64 GUI 크로스 빌드입니다. 실제 Windows 창/GPU/IME/클립보드/네트워크/DPAPI 실행은 미검증입니다.
- Linux 저장 시험은 테스트 전용 실제 AES-GCM 백엔드이며 DPAPI 실행 검증이 아닙니다. 비Windows production 저장은 DPAPI 미지원 오류로 fail closed합니다. macOS M1 실행 파일은 포함되지 않습니다.
- 미리보기는 실제 egui mesh 소프트웨어 렌더링이며 Windows OS 캡처가 아닙니다. 좁은 화면 표의 아래 행은 세로 스크롤합니다.

문제 제보 시 Windows 버전, 오류 문구, 창 크기와 위치를 알려주세요. URL/토큰/요청·응답의 민감정보는 제거하세요. licenses 및 BUILD-VERIFICATION.json을 실행 파일과 함께 보관하세요. 이전 배포본과 미리보기는 별도로 보존했습니다.
