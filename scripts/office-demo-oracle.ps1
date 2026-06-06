# Office P0 demo oracle — checks deliverables/ output against skill verify hints
# Usage:
#   .\scripts\office-demo-oracle.ps1 -WorkspaceRoot C:\path\to\workspace
#   .\scripts\office-demo-oracle.ps1 -WorkspaceRoot . -Scenario p0-4
#   .\scripts\office-demo-oracle.ps1 -WorkspaceRoot . -Scenario p0-3

param(
    [Parameter(Mandatory = $true)]
    [string]$WorkspaceRoot,
    [ValidateSet('p0-2', 'p0-3', 'p0-4', 'any')]
    [string]$Scenario = 'p0-2'
)

$ErrorActionPreference = 'Stop'

function Get-DocxPlainText {
    param([string]$Path)
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Path)
    try {
        $entry = $zip.Entries | Where-Object { $_.FullName -eq 'word/document.xml' } | Select-Object -First 1
        if (-not $entry) { return '' }
        $stream = $entry.Open()
        $reader = New-Object System.IO.StreamReader($stream)
        $xml = $reader.ReadToEnd()
        $reader.Close()
        $stream.Close()
        return ($xml -replace '<[^>]+>', ' ')
    }
    finally {
        $zip.Dispose()
    }
}

function Get-XlsxSheetNames {
    param([string]$Path)
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Path)
    try {
        return @(
            $zip.Entries |
                Where-Object { $_.FullName -like 'xl/worksheets/sheet*.xml' } |
                ForEach-Object { $_.FullName }
        )
    }
    finally {
        $zip.Dispose()
    }
}

$root = Resolve-Path $WorkspaceRoot
$deliverables = Join-Path $root 'deliverables'
if (-not (Test-Path $deliverables)) {
    Write-Error "Missing deliverables/ under $root"
    exit 1
}

$failures = 0

switch ($Scenario) {
    'p0-2' {
        $docx = Get-ChildItem -Path $deliverables -Filter '*.docx' -File |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1
        if (-not $docx) {
            Write-Host 'FAIL: no .docx in deliverables/' -ForegroundColor Red
            exit 1
        }
        Write-Host "Checking P0-2: $($docx.Name)" -ForegroundColor Cyan
        $text = Get-DocxPlainText -Path $docx.FullName
        foreach ($keyword in @('待决事项')) {
            if ($text -notmatch [regex]::Escape($keyword)) {
                Write-Host "FAIL: DOCX missing section keyword: $keyword" -ForegroundColor Red
                $failures++
            }
            else {
                Write-Host "OK: found '$keyword'" -ForegroundColor Green
            }
        }
    }
    'p0-3' {
        $docx = Get-ChildItem -Path $deliverables -Filter '*.docx' -File |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1
        if (-not $docx) {
            Write-Host 'FAIL: no .docx in deliverables/' -ForegroundColor Red
            exit 1
        }
        Write-Host "Checking P0-3: $($docx.Name)" -ForegroundColor Cyan
        $text = Get-DocxPlainText -Path $docx.FullName
        foreach ($keyword in @('概况', 'OEE')) {
            if ($text -notmatch [regex]::Escape($keyword)) {
                Write-Host "FAIL: DOCX missing keyword: $keyword" -ForegroundColor Red
                $failures++
            }
            else {
                Write-Host "OK: found '$keyword'" -ForegroundColor Green
            }
        }
    }
    'p0-4' {
        $xlsx = Get-ChildItem -Path $deliverables -Filter '*.xlsx' -File |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1
        if (-not $xlsx) {
            Write-Host 'FAIL: no .xlsx in deliverables/' -ForegroundColor Red
            exit 1
        }
        Write-Host "Checking P0-4: $($xlsx.Name)" -ForegroundColor Cyan
        $sheets = Get-XlsxSheetNames -Path $xlsx.FullName
        if ($sheets.Count -lt 1) {
            Write-Host 'FAIL: XLSX has no worksheet entries' -ForegroundColor Red
            $failures++
        }
        else {
            Write-Host "OK: XLSX has $($sheets.Count) sheet(s)" -ForegroundColor Green
        }
    }
    'any' {
        $latest = Get-ChildItem -Path $deliverables -File |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1
        if (-not $latest) {
            Write-Host 'FAIL: deliverables/ is empty' -ForegroundColor Red
            exit 1
        }
        Write-Host "OK: latest deliverable $($latest.Name)" -ForegroundColor Green
    }
}

if ($failures -gt 0) {
    Write-Host "Oracle failed with $failures issue(s)." -ForegroundColor Red
    exit 1
}

Write-Host 'Oracle passed.' -ForegroundColor Green
exit 0
