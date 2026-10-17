# Downloads the PDFium library the PDF tools bind at run time (bblanchon/pdfium-binaries,
# chromium/8100), checks its SHA-256 and unpacks it into desktop/vendor/windows/pdfium/, the
# folder debug builds look in (src/engine/pdf.rs). The archive's LICENSE and licenses/ come
# along: they must ship with the library. CI runs this before the tests; run it from anywhere.
$ErrorActionPreference = 'Stop'

$release = 'chromium%2F8100'
$expected = '51621bcc1d9be9ea448f4b84b0516862b928e3968d3fca899af1a6d6e2848d6d'
$desktop = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $desktop 'vendor/windows/pdfium'
$work = Join-Path ([IO.Path]::GetTempPath()) ("pdfium-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $archive = Join-Path $work 'pdfium-win-x64.tgz'
    $url = "https://github.com/bblanchon/pdfium-binaries/releases/download/$release/pdfium-win-x64.tgz"
    Write-Host "Downloading $url"
    Invoke-WebRequest -Uri $url -OutFile $archive -UserAgent 'offkit-build-check'

    $actual = (Get-FileHash -Algorithm SHA256 -Path $archive).Hash.ToLowerInvariant()
    if ($actual -ne $expected) {
        throw "Refusing pdfium-win-x64.tgz: its SHA-256 is $actual, expected $expected."
    }

    $extracted = Join-Path $work 'x'
    New-Item -ItemType Directory -Path $extracted | Out-Null
    tar -xzf $archive -C $extracted
    if ($LASTEXITCODE -ne 0) { throw 'tar could not unpack the archive.' }
    $dll = Join-Path $extracted 'bin/pdfium.dll'
    if (-not (Test-Path $dll)) { throw 'The archive has no pdfium.dll in bin/.' }

    if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
    New-Item -ItemType Directory -Path $dest | Out-Null
    Copy-Item $dll $dest
    Copy-Item (Join-Path $extracted 'LICENSE'), (Join-Path $extracted 'VERSION') $dest
    Copy-Item -Recurse (Join-Path $extracted 'licenses') $dest
    Write-Host "PDFium chromium/8100 (win-x64) is in $dest"
}
finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
