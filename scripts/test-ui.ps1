# SeeClaw Frontend Test Script
# Usage: .\scripts\test-ui.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Write-Host "Running frontend tests..." -ForegroundColor Cyan

Push-Location "src-ui"
yarn test
Pop-Location

Write-Host "Frontend tests complete!" -ForegroundColor Green
