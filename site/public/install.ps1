& {
    $url = 'https://github.com/usecornercase/cornercase-terminal/releases/latest/download/cornercase-installer.ps1'
    Invoke-Expression (Invoke-RestMethod -Uri $url -ErrorAction Stop)
}
