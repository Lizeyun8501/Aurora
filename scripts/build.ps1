# ============================================================================
# Aurora Note 一键构建（Windows PowerShell）
#
# 用法:
#   .\scripts\build.ps1                 # Debug APK（默认）
#   .\scripts\build.ps1 -Release        # Release APK
#   .\scripts\build.ps1 -SkipFrontend   # 跳过前端（复用 dist/）
#   .\scripts\build.ps1 -Clean          # 清理 FFI 产物后重编
#   .\scripts\build.ps1 -Check          # 仅环境自检
#
# 链路: 前端 vite → FFI 交叉编译(NDK) → jniLibs → gradle APK
# 产物: apps\mobile\android\app\build\outputs\apk\{debug|release}\
#
# 前置（一次性）:
#   - Rust 1.91+ (rustup), rustup target add aarch64-linux-android
#   - Android Studio 或命令行 SDK（含 NDK 26+）
#   - JDK 17/21, Node 18+
# ============================================================================
[CmdletBinding()]
param(
    [switch]$Release,
    [switch]$SkipFrontend,
    [switch]$Clean,
    [switch]$Check
)
$ErrorActionPreference = "Stop"

$Mode = if ($Release) { "release" } else { "debug" }

# ---------- 常量 ----------
$RepoRoot = (Resolve-Path "$PSScriptRoot\..").Path
$MobileDir = Join-Path $RepoRoot "apps\mobile"
$AndroidDir = Join-Path $MobileDir "android"
$JniDir = Join-Path $AndroidDir "app\src\main\jniLibs\arm64-v8a"
$AssetsDir = Join-Path $AndroidDir "app\src\main\assets"

# ---------- 环境探测 ----------
function Find-AndroidSdk {
    if ($env:ANDROID_HOME -and (Test-Path "$($env:ANDROID_HOME)\ndk")) { return $env:ANDROID_HOME }
    if ($env:ANDROID_SDK_ROOT -and (Test-Path "$($env:ANDROID_SDK_ROOT)\ndk")) { return $env:ANDROID_SDK_ROOT }
    foreach ($d in @(
        "$env:LOCALAPPDATA\Android\Sdk",
        "$env:USERPROFILE\AppData\Local\Android\Sdk",
        "C:\Android\Sdk"
    )) {
        if (Test-Path "$d\ndk") { return $d }
    }
    return $null
}

$SdkRoot = Find-AndroidSdk
if (-not $SdkRoot) {
    Write-Host "❌ Android SDK 未找到（设 ANDROID_HOME 或 Android Studio 默认路径）" -ForegroundColor Red
    exit 1
}

# NDK 取最高版本
$NdkVersion = (Get-ChildItem "$SdkRoot\ndk" -Directory | Sort-Object Name | Select-Object -Last 1).Name
if (-not $NdkVersion) {
    Write-Host "❌ NDK 未安装: $SdkRoot\ndk" -ForegroundColor Red
    exit 1
}
$NdkRoot = Join-Path $SdkRoot "ndk\$NdkVersion"
$Toolchain = Join-Path $NdkRoot "toolchains\llvm\prebuilt\windows-x86_64"
$ApiLevel = 24
# Windows 下 NDK clang 是 .cmd 包装器
$Clang = Join-Path $Toolchain "bin\aarch64-linux-android$ApiLevel-clang.cmd"

# Java
function Find-Java {
    if ($env:JAVA_HOME -and (Test-Path "$($env:JAVA_HOME)\bin\java.exe")) { return $env:JAVA_HOME }
    foreach ($d in @("C:\Program Files\Java\jdk-21*", "C:\Program Files\Java\jdk-17*",
                     "C:\Program Files\Eclipse Adoptium\jdk-21*", "$env:USERPROFILE\.jdks\*")) {
        $hit = Get-Item $d -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($hit -and (Test-Path "$($hit.FullName)\bin\java.exe")) { return $hit.FullName }
    }
    return $null
}
$JavaHome = Find-Java

# Gradle: 项目 wrapper 优先
$GradleCmd = $null
if (Test-Path "$AndroidDir\gradlew.bat") {
    $GradleCmd = "$AndroidDir\gradlew.bat"
} elseif (Get-Command gradle -ErrorAction SilentlyContinue) {
    $GradleCmd = "gradle"
} else {
    Write-Host "❌ Gradle 未找到（建议项目加 wrapper: gradle wrapper）" -ForegroundColor Red
    exit 1
}

# ---------- 自检 ----------
Write-Host "════════════════════════════════════════════════"
Write-Host " Aurora 一键构建（$Mode）"
Write-Host "════════════════════════════════════════════════"
Write-Host " SDK:      $SdkRoot"
Write-Host " NDK:      $NdkVersion"
Write-Host " clang:    $Clang"
if ($JavaHome) { Write-Host " JAVA:     $JavaHome" }
Write-Host " Gradle:   $GradleCmd"
Write-Host "════════════════════════════════════════════════"

if (-not (Test-Path $Clang)) {
    # 某些 NDK 版本用无扩展名 clang（cmd 包装在 bin\ 下同名 .cmd）
    $alt = Join-Path $Toolchain "bin\aarch64-linux-android$ApiLevel-clang"
    if (Test-Path $alt) { $Clang = $alt } else {
        Write-Host "❌ NDK clang 缺失: $Clang" -ForegroundColor Red; exit 1
    }
}
foreach ($tool in @("node", "cargo")) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        Write-Host "❌ $tool 缺失" -ForegroundColor Red; exit 1
    }
}
Write-Host "✅ 环境自检通过" -ForegroundColor Green

# Rust target
if (-not (rustup target list --installed | Select-String "aarch64-linux-android")) {
    Write-Host "📦 安装 aarch64-linux-android target..."
    rustup target add aarch64-linux-android
}

if ($Check) { Write-Host "（-Check 完成）"; exit 0 }

Set-Location $RepoRoot

# ---------- 1. 前端 ----------
if (-not $SkipFrontend) {
    Write-Host "── [1/3] 前端构建（vite）──"
    Push-Location $MobileDir
    npx vite build
    Pop-Location
    New-Item -ItemType Directory -Force -Path $AssetsDir | Out-Null
    Copy-Item "$MobileDir\dist\index.html" "$AssetsDir\index.html" -Force
} else {
    Write-Host "── [1/3] 跳过前端（复用 dist/）──"
    if (-not (Test-Path "$MobileDir\dist\index.html")) {
        Write-Host "❌ dist\index.html 不存在，去掉 -SkipFrontend" -ForegroundColor Red; exit 1
    }
}

# ---------- 2. FFI 交叉编译 ----------
Write-Host "── [2/3] Rust FFI 交叉编译（aarch64, $Mode）──"
if ($Clean) { cargo clean -p aurora-mobile-ffi }

# Windows NDK clang 的 .cmd 包装有参数转义问题 — 用 cargo config linker 兜底:
# 环境变量路径含空格时 rustc 需要 -C link-arg 处理; 此处直接设 CC/linker env
$env:CC_aarch64_linux_android = $Clang
$env:AR_aarch64_linux_android = Join-Path $Toolchain "bin\llvm-ar.exe"
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $Clang
$env:CARGO_INCREMENTAL = "0"

$ffiFlag = if ($Release) { "--release" } else { "" }
cargo build -p aurora-mobile-ffi --features p2p-sync $ffiFlag --target aarch64-linux-android
if ($LASTEXITCODE -ne 0) { Write-Host "❌ FFI 编译失败" -ForegroundColor Red; exit 1 }

$soSrc = Join-Path $RepoRoot "target\aarch64-linux-android\$Mode\libaurora_mobile_ffi.so"
if (-not (Test-Path $soSrc)) {
    Write-Host "❌ FFI 产物缺失: $soSrc" -ForegroundColor Red; exit 1
}
New-Item -ItemType Directory -Force -Path $JniDir | Out-Null
Copy-Item $soSrc "$JniDir\libaurora_mobile_ffi.so" -Force
Write-Host "   → jniLibs\arm64-v8a\libaurora_mobile_ffi.so ($([math]::Round((Get-Item $soSrc).Length/1MB,1)) MB)"

# ---------- 3. APK ----------
Write-Host "── [3/3] Gradle APK（$Mode）──"
$gradleTask = if ($Release) { "assembleRelease" } else { "assembleDebug" }
Push-Location $AndroidDir
$env:ANDROID_HOME = $SdkRoot
if ($JavaHome) { $env:JAVA_HOME = $JavaHome }

# local.properties 兜底
$localProps = "local.properties"
$needSdkDir = -not ((Test-Path $localProps) -and (Get-Content $localProps | Select-String "sdk.dir"))
if ($needSdkDir) {
    # properties 文件路径需正斜杠或转义反斜杠
    Add-Content $localProps "sdk.dir=$(($SdkRoot -replace '\\','\\'))"
}

if ($GradleCmd -eq "gradle") {
    gradle $gradleTask --no-daemon
} else {
    & $GradleCmd $gradleTask --no-daemon
}
$gradleExit = $LASTEXITCODE
Pop-Location
if ($gradleExit -ne 0) { Write-Host "❌ Gradle 失败" -ForegroundColor Red; exit 1 }

$apk = Join-Path $AndroidDir "app\build\outputs\apk\$Mode\app-$Mode.apk"
Write-Host "════════════════════════════════════════════════"
if (Test-Path $apk) {
    Write-Host "✅ 构建成功: $apk ($([math]::Round((Get-Item $apk).Length/1MB,1)) MB)" -ForegroundColor Green
    Write-Host "   （release 需签名: apksigner + keystore）"
} else {
    Write-Host "❌ APK 未生成（查上方 gradle 日志）" -ForegroundColor Red
    exit 1
}
