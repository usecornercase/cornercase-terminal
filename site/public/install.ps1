& {
    $url = 'https://github.com/usecornercase/cornercase-terminal/releases/latest/download/cornercase-installer.ps1'

    $folder = if ($env:CORNERCASE_INSTALL_DIR) {
        $env:CORNERCASE_INSTALL_DIR
    } elseif ($env:CARGO_DIST_FORCE_INSTALL_DIR) {
        $env:CARGO_DIST_FORCE_INSTALL_DIR
    } elseif ($env:CORNERCASE_UNMANAGED_INSTALL) {
        $env:CORNERCASE_UNMANAGED_INSTALL
    } else {
        Join-Path $HOME '.local\bin'
    }
    $cargo = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME '.cargo' }
    if ($folder.Replace('\\', '\') -eq $cargo) {
        $folder = Join-Path $folder 'bin'
    }

    $exe = Join-Path $folder 'cornercase.exe'
    $aside = $null
    if (Test-Path -LiteralPath $exe -PathType Leaf) {
        $names = @('cornercase.old.exe') + (1..99 | ForEach-Object { "cornercase.old.$_.exe" })
        $aside = $names | ForEach-Object { Join-Path $folder $_ } |
            Where-Object { -not (Test-Path -LiteralPath $_) } | Select-Object -First 1
        if (-not $aside) {
            throw "cornercase was not installed: 100 old copies of $exe are still in $folder"
        }
    }

    $installer = Join-Path ([IO.Path]::GetTempPath()) "cornercase-installer-$([guid]::NewGuid()).ps1"
    try {
        if ($aside) {
            Move-Item -LiteralPath $exe -Destination $aside -ErrorAction Stop
        }
        $text = Invoke-RestMethod -Uri $url -ErrorAction Stop
        [IO.File]::WriteAllText($installer, $text, (New-Object Text.UTF8Encoding $true))
        $global:LASTEXITCODE = 0
        & $installer
    } finally {
        Remove-Item -LiteralPath $installer -ErrorAction SilentlyContinue
        if ($aside -and -not (Test-Path -LiteralPath $exe)) {
            Move-Item -LiteralPath $aside -Destination $exe
        }
    }
    if ($LASTEXITCODE -ne 0) {
        throw 'cornercase was not installed'
    }
}
