set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default:
    @just --list

# ---- Configuration ----
METHODS_PATH := "program_methods"
TEST_METHODS_PATH := "test_program_methods"
ARTIFACTS := "artifacts"

# ---- Artifacts build ----
build-artifacts:
    @echo "🔨 Building artifacts"
    @for methods_path in {{METHODS_PATH}} {{TEST_METHODS_PATH}}; do \
        echo "Building artifacts for $methods_path"; \
        CARGO_TARGET_DIR=target/$methods_path cargo risczero build --manifest-path $methods_path/guest/Cargo.toml; \
        mkdir -p {{ARTIFACTS}}/$methods_path; \
        cp target/$methods_path/riscv32im-risc0-zkvm-elf/docker/*.bin {{ARTIFACTS}}/$methods_path; \
    done
