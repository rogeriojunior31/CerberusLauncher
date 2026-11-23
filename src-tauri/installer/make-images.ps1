# Draws the installer's pictures (NSIS wants 24-bit BMPs) from the app icon.
# Run again after changing the icon or the version shown:  pwsh src-tauri/installer/make-images.ps1
param([string]$Version = "0.10")
Add-Type -AssemblyName System.Drawing
$here = $PSScriptRoot
$icon = [System.Drawing.Image]::FromFile((Join-Path $here "..\icons\icon.png"))
$bg = [System.Drawing.Color]::FromArgb(11, 13, 19)
$ink = [System.Drawing.Color]::FromArgb(244, 245, 248)
$muted = [System.Drawing.Color]::FromArgb(167, 173, 186)
$orange = [System.Drawing.Color]::FromArgb(255, 154, 61)
$red = [System.Drawing.Color]::FromArgb(255, 61, 77)

function New-Canvas([int]$w, [int]$h) {
  $bmp = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = 'AntiAlias'; $g.InterpolationMode = 'HighQualityBicubic'; $g.TextRenderingHint = 'AntiAliasGridFit'
  $g.Clear($bg)
  return $bmp, $g
}

# Side picture of the welcome and finish pages: 164 x 314.
$bmp, $g = New-Canvas 164 314
$glow = New-Object System.Drawing.Drawing2D.LinearGradientBrush (New-Object System.Drawing.Point 0, 0), (New-Object System.Drawing.Point 164, 314), ([System.Drawing.Color]::FromArgb(70, 255, 90, 60)), ([System.Drawing.Color]::FromArgb(0, 11, 13, 19))
$g.FillRectangle($glow, 0, 0, 164, 314)
$g.DrawImage($icon, 22, 40, 120, 120)
$title = New-Object System.Drawing.Font "Segoe UI", 15, ([System.Drawing.FontStyle]::Bold)
$small = New-Object System.Drawing.Font "Segoe UI", 9
$center = New-Object System.Drawing.StringFormat; $center.Alignment = 'Center'
$g.DrawString("Cerberus", $title, (New-Object System.Drawing.SolidBrush $ink), (New-Object System.Drawing.RectangleF 0, 178, 164, 30), $center)
$g.DrawString("Launcher", $title, (New-Object System.Drawing.SolidBrush $ink), (New-Object System.Drawing.RectangleF 0, 204, 164, 30), $center)
$bar = New-Object System.Drawing.Drawing2D.LinearGradientBrush (New-Object System.Drawing.Point 52, 0), (New-Object System.Drawing.Point 112, 0), $orange, $red
$g.FillRectangle($bar, 52, 242, 60, 3)
$g.DrawString("versão $Version", $small, (New-Object System.Drawing.SolidBrush $muted), (New-Object System.Drawing.RectangleF 0, 256, 164, 20), $center)
$g.Dispose(); $bmp.Save((Join-Path $here "sidebar.bmp"), [System.Drawing.Imaging.ImageFormat]::Bmp); $bmp.Dispose()

# Header of the other pages: 150 x 57, on white like the rest of the page.
$bmp = New-Object System.Drawing.Bitmap 150, 57, ([System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = 'AntiAlias'; $g.InterpolationMode = 'HighQualityBicubic'; $g.TextRenderingHint = 'AntiAliasGridFit'
$g.Clear([System.Drawing.Color]::White)
$g.DrawImage($icon, 104, 7, 43, 43)
$g.Dispose(); $bmp.Save((Join-Path $here "header.bmp"), [System.Drawing.Imaging.ImageFormat]::Bmp); $bmp.Dispose()
$icon.Dispose()
"ok"
