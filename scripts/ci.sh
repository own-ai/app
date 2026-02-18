#!/usr/bin/env bash

# Local CI Script - Runs all checks from .github/workflows/ci.yml
# Usage: ./scripts/ci.sh or pnpm ci:local

set -e  # Exit on any error

BLUE='\033[0;34m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

print_step() {
    echo ""
    echo -e "${BLUE}=====================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}=====================================${NC}"
}

print_success() {
    echo -e "${GREEN}✓ $1${NC}"
}

print_error() {
    echo -e "${RED}✗ $1${NC}"
}

# Frontend Checks
print_step "Frontend: TypeScript Type Check"
pnpm tsc --noEmit
print_success "TypeScript type check passed"

print_step "Frontend: ESLint"
pnpm lint
print_success "ESLint check passed"

print_step "Frontend: Prettier"
pnpm format:check
print_success "Prettier check passed"

# Backend Checks
print_step "Backend: Rust Formatting"
cd src-tauri
cargo fmt --check
print_success "Rust formatting check passed"

print_step "Backend: Clippy"
cargo clippy -- -D warnings
print_success "Clippy check passed"

print_step "Backend: Build"
cargo build
print_success "Build passed"

print_step "Backend: Tests"
cargo test
print_success "Tests passed"

if [ "$RUN_ALL_TESTS" = "1" ]; then
    print_step "Backend: Ignored Tests (fastembed, keychain, integration)"
    cargo test -- --ignored
    print_success "Ignored tests passed"
fi

cd ..

# Summary
echo ""
echo -e "${GREEN}=====================================${NC}"
echo -e "${GREEN}All CI checks passed successfully!${NC}"
echo -e "${GREEN}=====================================${NC}"
