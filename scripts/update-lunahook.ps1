# 从 LunaTranslator 的 GitHub Release 中只取钩子核心，放进 src-tauri/resources/lunahook/。
#
#   pnpm lunahook:update            # 最新版
#   pnpm lunahook:update v12.1.2.4  # 指定版本
#   pnpm lunahook:fetch             # 还原 VERSION 记录的版本（新克隆、CI 用）
#
# 只取 5 个文件（约 6 MB），不要整个 LunaTranslator（约 65 MB）：
#   LunaHost64.dll          宿主，Fukidashi 进程直接加载
#   LunaHook32/64.dll       注入到游戏里的钩子，按游戏位数选
#   LunaSubprocess32/64.exe 官方注入器（dllinject <pid> <dll>），按游戏位数选
param([string]$Tag = "latest")

$ErrorActionPreference = "Stop"
$repo = "HIllya51/LunaTranslator"
$dest = Join-Path $PSScriptRoot "..\src-tauri\resources\lunahook"

# pinned：按仓库里记录的 VERSION 拉取（二进制不进仓库，新克隆和 CI 都靠它还原）
if ($Tag -eq "pinned") { $Tag = (Get-Content (Join-Path $dest "VERSION") -Raw).Trim() }

$api = if ($Tag -eq "latest") { "releases/latest" } else { "releases/tags/$Tag" }
$headers = @{ "User-Agent" = "fukidashi" }
# CI 的共享出口 IP 很容易撞上匿名 API 的限流，有令牌就带上
if ($env:GITHUB_TOKEN) { $headers["Authorization"] = "Bearer $env:GITHUB_TOKEN" }
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/$api" -Headers $headers
$asset = $release.assets | Where-Object name -eq "LunaTranslator_x64.zip"
if (-not $asset) { throw "$($release.tag_name) 里没有 LunaTranslator_x64.zip" }

$tmp = Join-Path ([IO.Path]::GetTempPath()) "fukidashi-lunahook-$([guid]::NewGuid())"
New-Item -ItemType Directory $tmp | Out-Null
try {
    Write-Host "下载 $($release.tag_name) ($([math]::Round($asset.size / 1MB)) MB)..."
    $zip = Join-Path $tmp "luna.zip"
    Invoke-WebRequest $asset.browser_download_url -OutFile $zip -UseBasicParsing

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($zip)
    try {
        $want = "LunaHost64.dll", "LunaHook32.dll", "LunaHook64.dll", "LunaSubprocess32.exe", "LunaSubprocess64.exe"
        New-Item -ItemType Directory -Force $dest | Out-Null
        foreach ($name in $want) {
            $entry = $archive.Entries | Where-Object { $_.FullName -match "/files/(LunaHook/)?$([regex]::Escape($name))$" } | Select-Object -First 1
            if (-not $entry) { throw "发布包里找不到 $name，LunaTranslator 可能调整了目录结构" }
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $dest $name), $true)
        }
    } finally { $archive.Dispose() }

    Invoke-WebRequest "https://raw.githubusercontent.com/$repo/$($release.tag_name)/LICENSE" -OutFile (Join-Path $dest "COPYING") -UseBasicParsing
    Set-Content (Join-Path $dest "VERSION") $release.tag_name -Encoding ascii -NoNewline

    # LunaHost64.dll / LunaSubprocess64.exe 依赖 x64 VC++ 运行库。随包放一份（微软允许随应用分发），
    # 用户不用另装运行库。取本机 System32 里的 64 位版本。
    foreach ($name in "msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll") {
        Copy-Item (Join-Path $env:SystemRoot "System32\$name") (Join-Path $dest $name) -Force
    }
    Write-Host "已更新到 $($release.tag_name) -> $((Resolve-Path $dest).Path)"
} finally {
    Remove-Item -Recurse -Force $tmp
}
