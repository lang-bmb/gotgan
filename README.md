# 곳간 (Gotgan)

> BMB Package Manager with Rust Ecosystem Support

BMB 패키지 매니저. Rust 폴백 생태계를 지원하고 Rust→BMB 마이그레이션 도구를 제공합니다.

## Features

### Core Package Management
- `gotgan new <name>` - 새 프로젝트 생성
- `gotgan build` - 프로젝트 빌드
- `gotgan run` - 빌드 및 실행
- `gotgan test` - 테스트 실행
- `gotgan verify` - 계약 검증 (BMB)
- `gotgan publish` - 패키지 배포

### Rust Fallback Ecosystem
- **Cargo 호환**: Rust crates를 의존성으로 사용 가능
- **FFI 자동 생성**: Rust 라이브러리 바인딩 자동 생성
- **혼합 프로젝트**: BMB + Rust 코드 동시 사용

### Rust to BMB Migration
- `gotgan migrate <crate>` - Rust crate를 BMB로 변환
- **점진적 마이그레이션**: 함수 단위로 순차 변환
- **계약 추론**: Rust 코드에서 pre/post 조건 추론

## Installation

```bash
# From source (requires Rust)
cargo install gotgan

# Pre-built binary (planned)
curl -sSf https://bmb-lang.org/gotgan/install.sh | sh
```

## Quick Start

```bash
# Create new project
gotgan new hello
cd hello

# Build and run
gotgan run
```

## Project Structure

```
hello/
├── gotgan.toml          # Project manifest
├── src/
│   └── main.bmb         # Entry point
└── tests/
    └── test_main.bmb    # Tests
```

## gotgan.toml

```toml
[package]
name = "hello"
version = "0.1.0"
edition = "2025"

[dependencies]
# BMB packages
json = "0.1"

# Rust fallback (crates.io)
[dependencies.rust]
serde = "1.0"
tokio = { version = "1", features = ["full"] }

[dev-dependencies]
test-utils = "0.1"
```

## Rust Interop

### Using Rust Crates

```toml
# gotgan.toml
[dependencies.rust]
regex = "1.10"
```

```bmb
-- main.bmb
use rust::regex::Regex;

fn match_email(s: &str) -> bool =
  Regex::new(r"^\S+@\S+\.\S+$").unwrap().is_match(s);
```

### Migration Example

```bash
# Analyze Rust crate for migration
gotgan migrate --analyze my_crate

# Generate BMB skeleton with inferred contracts
gotgan migrate --generate my_crate

# Verify migrated code
gotgan verify src/
```

## Roadmap

| Version | Features |
|---------|----------|
| v0.1 | Project structure, build, run |
| v0.2 | Local dependencies, lock file |
| v0.3 | Registry, publish, search |
| v0.4 | Rust fallback ecosystem |
| v0.5 | Migration tools |
| v1.0 | Full feature set, BMB rewrite |

## Development

```bash
# Build
cargo build

# Test
cargo test

# Run locally
cargo run -- new test-project
```

## License

MIT License
