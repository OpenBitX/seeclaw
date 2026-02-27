# SeeClaw Lint Script
# Usage: .\scripts\lint.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Write-Host "Running linters..." -ForegroundColor Cyan

# Rust lints
Write-Host "Running cargo clippy..." -ForegroundColor Yellow
cargo clippy --all-targets --all-features -- -D warnings

# Frontend lints
Write-Host "Running ESLint..." -ForegroundColor Yellow
Push-Location "src-ui"
yarn lint
Pop-Location

Write-Host "All lint checks passed!" -ForegroundColor Green
