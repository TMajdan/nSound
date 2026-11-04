# Generates assets/nsound.ico (multi-size) and assets/nsound-64.png.
# Run from the project root:  powershell -ExecutionPolicy Bypass -File assets\make-icon.ps1

Add-Type -AssemblyName System.Drawing

$outDir = Join-Path (Split-Path -Parent $PSScriptRoot) 'assets'
if (-not (Test-Path $outDir)) { New-Item -ItemType Directory -Path $outDir | Out-Null }

function New-RoundedPath {
    param([single]$x, [single]$y, [single]$w, [single]$h, [single]$r)
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = [single]($r * 2)
    $path.AddArc($x, $y, $d, $d, 180, 90)
    $path.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $path.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $path.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    return $path
}

function New-IconBitmap {
    param([int]$size)

    $scale = 4
    $big = $size * $scale
    $canvas = New-Object System.Drawing.Bitmap $big, $big
    $g = [System.Drawing.Graphics]::FromImage($canvas)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    # Rounded square background with a blue gradient.
    $pad = [single]($big * 0.035)
    $rect = New-Object System.Drawing.RectangleF $pad, $pad, ($big - 2 * $pad), ($big - 2 * $pad)
    $path = New-RoundedPath $rect.X $rect.Y $rect.Width $rect.Height ([single]($big * 0.22))
    $brush = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
        $rect,
        [System.Drawing.Color]::FromArgb(255, 108, 122, 255),
        [System.Drawing.Color]::FromArgb(255, 68, 78, 210),
        45.0
    )
    $g.FillPath($brush, $path)

    # White pause bars.
    $barWidth = [single]($big * 0.125)
    $barHeight = [single]($big * 0.40)
    $gap = [single]($big * 0.105)
    $totalWidth = 2 * $barWidth + $gap
    $left = [single](($big - $totalWidth) / 2)
    $top = [single](($big - $barHeight) / 2)
    $white = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::White)

    for ($i = 0; $i -lt 2; $i++) {
        $x = [single]($left + $i * ($barWidth + $gap))
        $bar = New-RoundedPath $x $top $barWidth $barHeight ([single]($barWidth / 2))
        $g.FillPath($white, $bar)
        $bar.Dispose()
    }

    $white.Dispose()
    $brush.Dispose()
    $path.Dispose()
    $g.Dispose()

    # Downscale for smooth edges.
    $result = New-Object System.Drawing.Bitmap $size, $size
    $scaler = [System.Drawing.Graphics]::FromImage($result)
    $scaler.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $scaler.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $scaler.DrawImage($canvas, 0, 0, $size, $size)
    $scaler.Dispose()
    $canvas.Dispose()
    return $result
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$entries = New-Object System.Collections.ArrayList

foreach ($size in $sizes) {
    $bitmap = New-IconBitmap $size
    if ($size -eq 64) {
        $bitmap.Save((Join-Path $outDir 'nsound-64.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    }
    $stream = New-Object System.IO.MemoryStream
    $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    [void]$entries.Add([pscustomobject]@{ Size = $size; Bytes = $stream.ToArray() })
    $stream.Dispose()
    $bitmap.Dispose()
}

$icoPath = Join-Path $outDir 'nsound.ico'
$file = [System.IO.File]::Create($icoPath)
$writer = New-Object System.IO.BinaryWriter($file)

$writer.Write([uint16]0)                 # reserved
$writer.Write([uint16]1)                 # type: icon
$writer.Write([uint16]$entries.Count)

$offset = 6 + 16 * $entries.Count
foreach ($entry in $entries) {
    $dimension = if ($entry.Size -ge 256) { 0 } else { $entry.Size }
    $writer.Write([byte]$dimension)
    $writer.Write([byte]$dimension)
    $writer.Write([byte]0)               # palette size
    $writer.Write([byte]0)               # reserved
    $writer.Write([uint16]1)             # colour planes
    $writer.Write([uint16]32)            # bits per pixel
    $writer.Write([uint32]$entry.Bytes.Length)
    $writer.Write([uint32]$offset)
    $offset += $entry.Bytes.Length
}
foreach ($entry in $entries) {
    $writer.Write($entry.Bytes)
}

$writer.Flush()
$writer.Dispose()
$file.Dispose()

Write-Output "wrote $icoPath ($($entries.Count) sizes) and nsound-64.png"
