# Build synthetic encrypted Easy Save 3 files with .NET, the same classes ES3 uses:
#   file = iv[16] || AES-128-CBC(key, iv, PKCS7)(payload)
#   key  = Rfc2898DeriveBytes(password, salt = iv, 100 iterations, HMAC-SHA1).GetBytes(16)
#   payload = UTF-8 JSON, or GZip(JSON) when compression is on (compress first, then encrypt).
# The IV is fixed so the files are reproducible. Password: synthetic-test-key (not from any game).
# Usage: powershell -ExecutionPolicy Bypass -File fixtures/tools/make_es3_aes.ps1 <output dir>
param([Parameter(Mandatory = $true)][string]$OutDir)

$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$password = 'synthetic-test-key'

function Protect-Es3([byte[]]$payload, [byte[]]$iv) {
    $kdf = New-Object System.Security.Cryptography.Rfc2898DeriveBytes($password, $iv, 100)
    $aes = [System.Security.Cryptography.Aes]::Create()
    $aes.Mode = [System.Security.Cryptography.CipherMode]::CBC
    $aes.Padding = [System.Security.Cryptography.PaddingMode]::PKCS7
    $aes.KeySize = 128
    $aes.Key = $kdf.GetBytes(16)
    $aes.IV = $iv
    $enc = $aes.CreateEncryptor()
    $cipher = $enc.TransformFinalBlock($payload, 0, $payload.Length)
    return [byte[]]($iv + $cipher)
}

function Compress-Gzip([byte[]]$data) {
    $ms = New-Object System.IO.MemoryStream
    $gz = New-Object System.IO.Compression.GZipStream($ms, [System.IO.Compression.CompressionMode]::Compress)
    $gz.Write($data, 0, $data.Length)
    $gz.Close()
    return $ms.ToArray()
}

$settings = '{"musicVolume":{"__type":"float","value":0.75},"fullscreen":{"__type":"bool","value":true},"playerName":{"__type":"string","value":"Ada"}}'
$progress = @'
{
	"level" : {
		"__type" : "int",
		"value" : 12
	},
	"unlocked" : {
		"__type" : "System.Collections.Generic.List`1[[System.Int32, mscorlib, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089]],mscorlib",
		"value" : [1,2,3,5,8]
	}
}
'@

$utf8 = New-Object System.Text.UTF8Encoding($false)
$iv1 = [byte[]](0..15)
$iv2 = [byte[]](16..31)

[System.IO.File]::WriteAllBytes((Join-Path $OutDir 'settings.es3'), (Protect-Es3 $utf8.GetBytes($settings) $iv1))
[System.IO.File]::WriteAllBytes((Join-Path $OutDir 'progress-gzip.es3'), (Protect-Es3 (Compress-Gzip $utf8.GetBytes($progress)) $iv2))
Get-ChildItem $OutDir | Select-Object Name, Length
