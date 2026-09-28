# 生成 NSIS 安装界面品牌图（依据 AShell 功能特性设计：终端 / SSH / SFTP / 监控 / AI 助手）。
#   src-tauri/icons/nsis/installer-header.bmp   150 x 57  安装向导页头：淡蓝渐变 + ❯_ 提示符纹理 + 右侧 Logo
#   src-tauri/icons/nsis/installer-sidebar.bmp  164 x 314 欢迎页竖图：暗色面板 + Logo + 标题 + 品牌线 + 标语
# 产物为 24 位 BMP，尺寸被 NSIS 强校验，勿改。用 powershell -ExecutionPolicy Bypass -File gen-nsis-images.ps1 重新生成。

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

# 3x 超采样：先在大画布上绘制，再降采样到 NSIS 要求的精确尺寸，字形与边缘更锐利
$SS = 3

function Save-Supersampled([System.Drawing.Bitmap]$Big, [int]$W, [int]$H, [string]$OutPath) {
    $final = New-Object System.Drawing.Bitmap($W, $H, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $g2 = [System.Drawing.Graphics]::FromImage($final)
    $g2.InterpolationMode = 'HighQualityBicubic'
    $g2.PixelOffsetMode = 'HighQuality'
    # 越界采样改用镜像，避免 HighQualityBicubic 把边缘与透明(黑)混合产生 1px 暗色镶边
    $attr = New-Object System.Drawing.Imaging.ImageAttributes
    $attr.SetWrapMode([System.Drawing.Drawing2D.WrapMode]::TileFlipXY)
    $destRect = New-Object System.Drawing.Rectangle(0, 0, $W, $H)
    $g2.DrawImage($Big, $destRect, 0, 0, $Big.Width, $Big.Height, [System.Drawing.GraphicsUnit]::Pixel, $attr)
    $attr.Dispose()
    $g2.Dispose()
    $Big.Dispose()
    $final.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Bmp)
    $final.Dispose()
}

$root = Split-Path -Parent $PSScriptRoot
$srcPng = Join-Path $root "src-tauri/icons/icon.png"
$outDir = Join-Path $root "src-tauri/icons/nsis"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

function New-CardPath([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $r * 2
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

function Draw-Chevron([System.Drawing.Graphics]$g, [float]$x, [float]$y, [float]$size, [System.Drawing.Color]$color, [float]$width) {
    # 矢量绘制 ">" 提示符，避免字体缺字形
    $pen = New-Object System.Drawing.Pen($color, $width)
    $pen.StartCap = 'Round'; $pen.EndCap = 'Round'
    $g.DrawLine($pen, $x, $y, $x + $size * 0.55, $y + $size * 0.5)
    $g.DrawLine($pen, $x + $size * 0.55, $y + $size * 0.5, $x, $y + $size)
    $pen.Dispose()
}

function Remove-LogoTile([System.Drawing.Image]$src, [int]$size) {
    # 去除 Logo 的深色方块底：洪泛填充只清除与外部连通的中性深色像素，
    # 白环、彩色图案、AI 徽标内部的深色细节不受影响
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g2 = [System.Drawing.Graphics]::FromImage($bmp)
    $g2.InterpolationMode = 'HighQualityBicubic'
    $g2.DrawImage($src, 0, 0, $size, $size)
    $g2.Dispose()

    $rect = New-Object System.Drawing.Rectangle(0, 0, $size, $size)
    $data = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadWrite, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $stride = $data.Stride
    $len = $stride * $size
    $bytes = New-Object byte[] $len
    [System.Runtime.InteropServices.Marshal]::Copy($data.Scan0, $bytes, 0, $len)

    $stack = New-Object 'System.Collections.Generic.Stack[int]'
    foreach ($rel in @(@(0.05, 0.5), @(0.5, 0.05), @(0.05, 0.05), @(0.95, 0.05), @(0.05, 0.95), @(0.95, 0.5))) {
        $i = ([int]($rel[1] * $size) * $stride) + ([int]($rel[0] * $size) * 4)
        $stack.Push($i)
    }
    while ($stack.Count -gt 0) {
        $i = $stack.Pop()
        if ($bytes[$i + 3] -lt 40) { continue }
        $b = $bytes[$i]; $g = $bytes[$i + 1]; $r = $bytes[$i + 2]
        $mx = [Math]::Max($r, [Math]::Max($g, $b)); $mn = [Math]::Min($r, [Math]::Min($g, $b))
        if (($mx - $mn) -ge 42 -or (($r + $g + $b) / 3) -ge 78) { continue }
        $bytes[$i + 3] = 0
        $x = [int](($i % $stride) / 4); $y = [int]($i / $stride)
        if ($x -gt 0) { $stack.Push($i - 4) }
        if ($x -lt ($size - 1)) { $stack.Push($i + 4) }
        if ($y -gt 0) { $stack.Push($i - $stride) }
        if ($y -lt ($size - 1)) { $stack.Push($i + $stride) }
    }
    [System.Runtime.InteropServices.Marshal]::Copy($bytes, 0, $data.Scan0, $len)
    $bmp.UnlockBits($data)
    return $bmp
}

# ---------- 1. 页头 150 x 57 ----------
$W = 150; $H = 57
$bmp = New-Object System.Drawing.Bitmap(($W * $SS), ($H * $SS), [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.ScaleTransform($SS, $SS)
$g.SmoothingMode = 'AntiAlias'
$g.TextRenderingHint = 'AntiAliasGridFit'
$g.InterpolationMode = 'HighQualityBicubic'
$g.PixelOffsetMode = 'HighQuality'

# 左淡蓝右纯白的横向渐变，右缘与安装器文字区无缝衔接
$bgBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point(0, 0)), (New-Object System.Drawing.Point($W, 0)),
    [System.Drawing.Color]::FromArgb(255, 234, 240, 253), [System.Drawing.Color]::White)
$g.FillRectangle($bgBrush, 0, 0, $W, $H)

# 左侧若隐若现的 ❯_ 提示符纹理
Draw-Chevron $g 8 16 26 ([System.Drawing.Color]::FromArgb(110, 205, 216, 245)) 4
$g.FillRectangle((New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(110, 205, 216, 245))), 38, 41, 14, 4)
Draw-Chevron $g 50 21 14 ([System.Drawing.Color]::FromArgb(70, 205, 216, 245)) 3
$g.FillRectangle((New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(70, 205, 216, 245))), 66, 33, 8, 3)

# 右侧 Logo（去除方块底，浅底上只留彩色图案）
$logo = [System.Drawing.Image]::FromFile($srcPng)
$tileFree = Remove-LogoTile $logo 384
$logo.Dispose()
$g.DrawImage($tileFree, ($W - 52 - 3), (($H - 52) / 2), 52, 52)
$tileFree.Dispose()

$g.Dispose()
Save-Supersampled $bmp $W $H (Join-Path $outDir "installer-header.bmp")
Write-Host "生成 installer-header.bmp ($W x $H, ${SS}x 超采样)"

# ---------- 2. 侧边图 164 x 314 ----------
$W = 164; $H = 314
$bmp = New-Object System.Drawing.Bitmap(($W * $SS), ($H * $SS), [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.ScaleTransform($SS, $SS)
$g.SmoothingMode = 'AntiAlias'
$g.TextRenderingHint = 'AntiAliasGridFit'
$g.InterpolationMode = 'HighQualityBicubic'
$g.PixelOffsetMode = 'HighQuality'

# 加载 Logo 并采样其圆角方块底色，作为面板基色（网格扫描，过滤彩色图案/白色描边/高光，只留中性深色）
$logo = [System.Drawing.Image]::FromFile($srcPng)
$sampleBmp = New-Object System.Drawing.Bitmap($logo)
$sumR = 0; $sumG = 0; $sumB = 0; $cnt = 0
for ($iy = 0; $iy -lt 16; $iy++) {
    for ($ix = 0; $ix -lt 16; $ix++) {
        $c = $sampleBmp.GetPixel([int](($ix + 0.5) * $sampleBmp.Width / 16), [int](($iy + 0.5) * $sampleBmp.Height / 16))
        if ($c.A -gt 200) {
            $mx = [Math]::Max($c.R, [Math]::Max($c.G, $c.B))
            $mn = [Math]::Min($c.R, [Math]::Min($c.G, $c.B))
            $lum = ($c.R + $c.G + $c.B) / 3
            if (($mx - $mn) -lt 40 -and $lum -lt 70) { $sumR += $c.R; $sumG += $c.G; $sumB += $c.B; $cnt++ }
        }
    }
}
$sampleBmp.Dispose()
if ($cnt -eq 0) { $sumR = 32; $sumG = 32; $sumB = 36; $cnt = 1 }
$baseR = [int]($sumR / $cnt); $baseG = [int]($sumG / $cnt); $baseB = [int]($sumB / $cnt)

# 面板底：Logo 底色的纵向渐变（顶部微亮、底部压暗），与 Logo 方块融为一体
$topR = [Math]::Min(255, [int]($baseR + (255 - $baseR) * 0.12))
$topG = [Math]::Min(255, [int]($baseG + (255 - $baseG) * 0.12))
$topB = [Math]::Min(255, [int]($baseB + (255 - $baseB) * 0.12))
$botR = [int]($baseR * 0.55); $botG = [int]($baseG * 0.55); $botB = [int]($baseB * 0.55)
$bgBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point(0, 0)), (New-Object System.Drawing.Point(0, $H)),
    [System.Drawing.Color]::FromArgb(255, $topR, $topG, $topB), [System.Drawing.Color]::FromArgb(255, $botR, $botG, $botB))
$g.FillRectangle($bgBrush, 0, 0, $W, $H)

# Logo 背后的柔和光晕（中性冷光，不与底色打架）
$glowPath = New-Object System.Drawing.Drawing2D.GraphicsPath
$glowPath.AddEllipse(17, 39, 130, 130)
$glowBrush = New-Object System.Drawing.Drawing2D.PathGradientBrush($glowPath)
$glowBrush.CenterColor = [System.Drawing.Color]::FromArgb(60, 155, 170, 210)
$glowBrush.SurroundColors = @([System.Drawing.Color]::FromArgb(0, 155, 170, 210))
$g.FillPath($glowBrush, $glowPath)
$glowBrush.Dispose(); $glowPath.Dispose()

# 底部深处的微光，避免大面积死黑
$auroraPath = New-Object System.Drawing.Drawing2D.GraphicsPath
$auroraPath.AddEllipse(-40, 260, 244, 160)
$auroraBrush = New-Object System.Drawing.Drawing2D.PathGradientBrush($auroraPath)
$auroraBrush.CenterColor = [System.Drawing.Color]::FromArgb(36, 120, 130, 160)
$auroraBrush.SurroundColors = @([System.Drawing.Color]::FromArgb(0, 120, 130, 160))
$g.FillPath($auroraBrush, $auroraPath)
$auroraBrush.Dispose(); $auroraPath.Dispose()

# Logo（去除方块底）+ 应用名：放大到 120 保持视觉分量
$tileFree = Remove-LogoTile $logo 384
$logo.Dispose()
$g.DrawImage($tileFree, (($W - 120) / 2), 44, 120, 120)
$tileFree.Dispose()

$fontTitle = New-Object System.Drawing.Font('Segoe UI', 23, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)

$sfCenter = New-Object System.Drawing.StringFormat
$sfCenter.Alignment = 'Center'
$white = [System.Drawing.Color]::FromArgb(255, 235, 238, 246)

$g.DrawString('AShell', $fontTitle, (New-Object System.Drawing.SolidBrush($white)),
    (New-Object System.Drawing.RectangleF(0, 172, $W, 30)), $sfCenter)

# 标题下的品牌渐变细线，呼应 Logo 配色
$divBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    (New-Object System.Drawing.Point(64, 214)), (New-Object System.Drawing.Point(100, 214)),
    [System.Drawing.Color]::FromArgb(150, 84, 134, 255), [System.Drawing.Color]::FromArgb(150, 46, 204, 113))
$g.FillRectangle($divBrush, 64, 214, 36, 2)
$divBrush.Dispose()

# 标语两行：特质词间隔点分组收敛，定位词更亮更大
$fontSlogan1 = New-Object System.Drawing.Font('Microsoft YaHei UI', 12, [System.Drawing.FontStyle]::Regular, [System.Drawing.GraphicsUnit]::Pixel)
$g.DrawString('高颜值 · 轻量 · 现代化', $fontSlogan1, (New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 168, 176, 198))),
    (New-Object System.Drawing.RectangleF(0, 230, $W, 17)), $sfCenter)
$fontSlogan2 = New-Object System.Drawing.Font('Microsoft YaHei UI', 14, [System.Drawing.FontStyle]::Regular, [System.Drawing.GraphicsUnit]::Pixel)
$g.DrawString('AI 智能终端', $fontSlogan2, (New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 228, 232, 243))),
    (New-Object System.Drawing.RectangleF(0, 254, $W, 19)), $sfCenter)

$g.Dispose()
Save-Supersampled $bmp $W $H (Join-Path $outDir "installer-sidebar.bmp")
Write-Host "生成 installer-sidebar.bmp ($W x $H, ${SS}x 超采样)"
