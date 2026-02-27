# SeeClaw Production Build Script
# Usage: .\scripts\build.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Write-Host "Building SeeClaw for production..." -ForegroundColor Cyan

# Verify Tauri CLI
if (-not (Get-Command "cargo-tauri" -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Tauri CLI..." -ForegroundColor Yellow
    cargo install tauri-cli --version "^2" --locked
}

# Ensure frontend deps are installed
if (-not (Test-Path "src-ui\node_modules")) {
    Write-Host "Installing frontend dependencies..." -ForegroundColor Yellow
    Push-Location "src-ui"
    yarn install
    Pop-Location
}

# Build frontend
Write-Host "Building frontend..." -ForegroundColor Yellow
Push-Location "src-ui"
yarn build
Pop-Location

# Build Tauri app (release mode)
Write-Host "Building Tauri application..." -ForegroundColor Yellow
cargo tauri build

Write-Host "Build complete! Installer located in target\release\bundle\" -ForegroundColor Green
