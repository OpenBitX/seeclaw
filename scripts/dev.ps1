# SeeClaw Development Script
# Usage: .\scripts\dev.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Write-Host "Starting SeeClaw in development mode..." -ForegroundColor Cyan

# Verify Rust toolchain
if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue)) {
    Write-Error "cargo not found. Please install Rust from https://rustup.rs/"
    exit 1
}

# Verify Tauri CLI
if (-not (Get-Command "cargo-tauri" -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Tauri CLI..." -ForegroundColor Yellow
    cargo install tauri-cli --version "^2" --locked
}

# Verify yarn
if (-not (Get-Command "yarn" -ErrorAction SilentlyContinue)) {
    Write-Error "yarn not found. Please install via: npm install -g yarn"
    exit 1
}

# Ensure frontend deps are installed
if (-not (Test-Path "src-ui\node_modules")) {
    Write-Host "Installing frontend dependencies..." -ForegroundColor Yellow
    Push-Location "src-ui"
    yarn install
    Pop-Location
}

# Start Tauri dev (builds Rust + starts Vite dev server)
cargo tauri dev
