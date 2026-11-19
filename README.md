# Imdev

Rust와 egui/eframe으로 작성된 네이티브 HTTP API 작업실입니다. 현재 소스 버전은 `Cargo.toml`을 기준으로 합니다.

## 빌드 및 테스트

Rust 도구 체인은 `rust-toolchain.toml`에 고정되어 있습니다. macOS에서는 Xcode Command Line Tools가 필요합니다.

```sh
cargo test --locked
cargo build --release --locked
./target/release/imdev --help
./target/release/imdev --headless-ui-check
python3 tools/package.py
```

패키징 도구는 Python 3.11 이상이 필요합니다. Python 도구 테스트는 Pillow 12.0.0 설치 후 다음 명령으로 실행합니다.

```sh
python3 -m unittest discover -s tests -p 'test_*.py' -v
```

## GitHub Actions macOS 빌드

저장소의 **Actions → macOS build → Run workflow**에서 수동 실행할 수 있습니다. 관련 소스의 `main` push 및 pull request에서도 실행됩니다.

- Apple Silicon: `macos-15`, `aarch64-apple-darwin`
- Intel: `macos-15-intel`, `x86_64-apple-darwin`
- 각 호스트에서 Rust 테스트, release 빌드, 창 없는 UI 검사, 앱 패키징을 실행합니다.
- ZIP 무결성, 버전, 실행 권한, 폰트 라이선스, SHA-256 및 압축에서 추출한 앱 번들의 엄격한 서명 검사를 실행합니다. LaunchServices를 통한 창 없는 실행도 검사합니다.
- 성공한 실행의 Artifacts에서 `Imdev-macOS-arm64` 또는 `Imdev-macOS-x86_64`를 받습니다. 보관 기간은 7일입니다.
- Artifacts 안에는 ad-hoc 서명된 `Imdev.app` ZIP, 소스 ZIP 및 `manifest.json`이 있습니다. 사용자 다운로드는 검증 후 게시하는 [GitHub Releases](https://github.com/kayinkr/imdev/releases)를 이용하세요.

표준 GitHub-hosted runner의 공개 저장소 실행은 GitHub의 무료 실행 대상입니다. 결과물 저장에는 별도 사용량 정책이 적용되며, 향후 저장소를 비공개로 바꾸면 실행 과금 조건도 달라집니다.

## 검증 범위 및 중요한 제약

**CI 빌드 성공은 실제 Mac에서의 GUI 사용 검증을 뜻하지 않습니다.** 창, GPU, 클립보드, 한글 IME 및 Gatekeeper 동작은 실제 Mac에서 별도로 확인해야 합니다.

앱은 Developer ID 서명과 Apple 공증을 하지 않습니다. 일반 사용자 배포용 설치 프로그램도 생성하지 않습니다. Apple 인증서나 비밀정보를 저장소에 올리지 마세요.

### macOS 실행 차단 및 0.3.2 패키징 수정

0.3.1 Apple Silicon 패키지는 실행 파일의 linker 서명만 가진 상태로 `.app`을 조립했습니다. 이 때문에 앱 검사에서 `code has no resources but signature indicates they must be present` 오류가 재현됐습니다. 0.3.2부터 최종 번들에 ad-hoc 서명을 적용하고, 압축을 다시 푼 번들을 `codesign --verify --deep --strict`로 검사합니다. plist 변조를 검출하는 macOS 회귀 테스트도 포함합니다.

**ad-hoc 서명의 유효성과 Gatekeeper의 배포 신뢰 승인은 다릅니다.** 인증서와 공증이 없으므로 Gatekeeper는 새 패키지도 거부할 수 있습니다. 공식 인증 배포본으로 오해하지 마세요. 출처와 체크섬을 확인하고 신뢰하는 경우에만 Apple이 안내하는 [시스템 설정의 개별 앱 열기 허용 절차](https://support.apple.com/en-us/102445)를 검토하세요. 허용 항목이 없거나 악성 소프트웨어 경고가 나타나면 우회하지 말고 진단 정보를 확인하세요. 시스템 전체 보안 기능을 끄지 마세요.

**현재 요청 저장용 암호화 구현은 Windows DPAPI 전용입니다.** macOS와 Linux에서는 암호화 저장 기능이 오류를 반환하며 평문 저장으로 우회하지 않습니다. 따라서 macOS 패키지가 빌드되더라도 요청 저장 기능까지 완성된 macOS 제품으로 간주하면 안 됩니다. macOS Keychain 기반 구현은 별도 작업입니다.

HTTP 테스트는 loopback mock 서버와 합성 데이터를 사용합니다. 공개 저장소에는 로컬 사용자 데이터, 인증정보, 빌드 캐시 및 이전 배포 파일을 포함하지 않습니다.

## 라이선스

프로젝트 메타데이터의 라이선스는 MIT입니다. 포함된 Noto Sans KR 폰트는 `assets/OFL-NotoSansKR.txt`의 SIL Open Font License를 따릅니다. 외부 의존성의 권리는 각 권리자에게 있습니다. 기존 Windows용 안내 문서는 `docs/README-Windows*.txt`에 보존되어 있습니다.
