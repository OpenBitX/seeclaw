# SeeClaw Build Script
# 用于打包 SeeClaw 应用

Write-Host "🚀 Starting SeeClaw build process..." -ForegroundColor Cyan

# 检查必需的文件
Write-Host "`n📋 Checking required files..." -ForegroundColor Yellow

$requiredFiles = @(
    "config.toml",
    "prompts/system/agent_system.md",
    "prompts/tools/builtin.json",
    "models/gpa_gui_detector.onnx"
)

$missingFiles = @()
foreach ($file in $requiredFiles) {
    if (-not (Test-Path $file)) {
        $missingFiles += $file
        Write-Host "  ❌ Missing: $file" -ForegroundColor Red
    } else {
        Write-Host "  ✅ Found: $file" -ForegroundColor Green
    }
}

if ($missingFiles.Count -gt 0) {
    Write-Host "`n⚠️  Warning: Some required files are missing." -ForegroundColor Red
    Write-Host "The build will continue, but the app may not work correctly." -ForegroundColor Yellow
    $continue = Read-Host "Continue anyway? (y/N)"
    if ($continue -ne 'y' -and $continue -ne 'Y') {
        Write-Host "Build cancelled." -ForegroundColor Red
        exit 1
    }
}

# 构建前端
Write-Host "`n🎨 Building frontend..." -ForegroundColor Cyan
Push-Location src-ui
yarn build
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Frontend build failed!" -ForegroundColor Red
    Pop-Location
    exit 1
}
Pop-Location
Write-Host "✅ Frontend build complete" -ForegroundColor Green

# 打包应用
Write-Host "`n📦 Building Tauri application..." -ForegroundColor Cyan
cargo tauri build

if ($LASTEXITCODE -ne 0) {
    Write-Host "`n❌ Build failed!" -ForegroundColor Red
    exit 1
}

Write-Host "`n✨ Build complete!" -ForegroundColor Green
Write-Host "`n📍 Build artifacts:" -ForegroundColor Cyan

# 查找并显示构建产物
$bundlePath = "target\release\bundle"
if (Test-Path $bundlePath) {
    Get-ChildItem -Path $bundlePath -Recurse -Include *.msi,*.exe | ForEach-Object {
        $size = [math]::Round($_.Length / 1MB, 2)
        Write-Host "  📦 $($_.FullName) ($size MB)" -ForegroundColor White
    }
}

Write-Host "`n✅ Build process finished successfully!" -ForegroundColor Green
Write-Host "`nNext steps:" -ForegroundColor Yellow
Write-Host "  1. Test the installer on a clean machine" -ForegroundColor White
Write-Host "  2. Verify all features work correctly" -ForegroundColor White
Write-Host "  3. Check that config.toml is created in the correct location" -ForegroundColor White
