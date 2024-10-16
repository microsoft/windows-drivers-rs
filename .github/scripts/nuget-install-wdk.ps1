param (
    [string]$wdkVersion
)

# Extract the first three parts of the version
$majorVersionNum = ($wdkVersion -split '\.')[0..2] -join '.'

Write-Host "Installing WDK version $wdkVersion"
Write-Host "Major version number: $majorVersionNum"

# Update the packages.config file with the version passed as parameter
$packagesConfigPath = "C:\WDK.config"

if (Test-Path $packagesConfigPath) {
    [xml]$packagesConfig = Get-Content $packagesConfigPath

    foreach ($package in $packagesConfig.packages.package) {
        if ($package.id -like "Microsoft.Windows.*") {
            $package.version = $wdkVersion
        }
    }

    $packagesConfig.Save($packagesConfigPath)
    Write-Host "Updated packages.config with version $wdkVersion"
} else {
    Write-Error "packages.config file not found"
}

# Install NuGet
try {
    nuget restore .\packages.config -PackagesDirectory C:\WDK
    Write-Host "WDK installed at C:\WDK"
} catch {
    Write-Error "Failed to restore packages using NuGet. Error: $_"
    exit 1
}

$folders = @(
    "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion",
    "C:\WDK\Microsoft.Windows.SDK.CPP.x64.$wdkVersion",
    "C:\WDK\Microsoft.Windows.SDK.CPP.arm64.$wdkVersion",
    "C:\WDK\Microsoft.Windows.WDK.ARM64.$wdkVersion",
    "C:\WDK\Microsoft.Windows.SDK.CPP.$wdkVersion"
)
foreach ($folder in $folders) {
if (-Not (Test-Path $folder)) {
        Write-Error "Required folder $folder is missing."
        exit 1
    }
}
function Copy-File {
    param (
        [string]$sourcePath,
        [string]$destinationPath,
        [string]$fileName
    )

    if (Test-Path $sourcePath) {
        Copy-Item -Path $sourcePath -Destination $destinationPath -Force
        Write-Host "Copied $fileName to $destinationPath"
    } else {
        Write-Error "$fileName not found at $sourcePath"
    }
}

# Copying signtool to WDK bin folder
$signtoolx64 = "C:\WDK\Microsoft.Windows.SDK.CPP.$wdkVersion\c\bin\$majorVersionNum.0\x64\signtool.exe"
$signtoolX86 = "C:\WDK\Microsoft.Windows.SDK.CPP.$wdkVersion\c\bin\$majorVersionNum.0\x86\signtool.exe"
$destinationx64 = "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\bin\$majorVersionNum.0\x64"
$destinationX86 = "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\bin\$majorVersionNum.0\x86"

Copy-File -sourcePath $signtoolX64 -destinationPath $destinationX64 -fileName "signtool.exe"
Copy-File -sourcePath $signtoolX86 -destinationPath $destinationX86 -fileName "signtool.exe"

# Copying certmgr to WDK bin folder
$certmgrx86 = "C:\WDK\Microsoft.Windows.SDK.CPP.$wdkVersion\c\bin\$majorVersionNum.0\x86\certmgr.exe"
$certmgrX64 = "C:\WDK\Microsoft.Windows.SDK.CPP.$wdkVersion\c\bin\$majorVersionNum.0\x64\certmgr.exe"
$certmgrARM64 = "C:\WDK\Microsoft.Windows.SDK.CPP.$wdkVersion\c\bin\$majorVersionNum.0\arm64\certmgr.exe"
$destinationx86 = "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\bin\$majorVersionNum.0\x86"
$destinationx64 = "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\bin\$majorVersionNum.0\x64"
$destinationARM64 = "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\bin\$majorVersionNum.0\ARM64"

Copy-File -sourcePath $certmgrx86 -destinationPath $destinationx86 -fileName "certmgr.exe"
Copy-File -sourcePath $certmgrX64 -destinationPath $destinationx64 -fileName "certmgr.exe"
Copy-File -sourcePath $certmgrARM64 -destinationPath $destinationARM64 -fileName "certmgr.exe"

# Copying makecert to WDK bin folder
$makecertx86 = "C:\WDK\Microsoft.Windows.SDK.CPP.$wdkVersion\c\bin\$majorVersionNum.0\x86\MakeCert.exe"
$makecertX64 = "C:\WDK\Microsoft.Windows.SDK.CPP.$wdkVersion\c\bin\$majorVersionNum.0\x64\MakeCert.exe"
$makecertARM64 = "C:\WDK\Microsoft.Windows.SDK.CPP.$wdkVersion\c\bin\$majorVersionNum.0\arm64\MakeCert.exe"
$destinationx86 = "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\bin\$majorVersionNum.0\x86"
$destinationx64 = "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\bin\$majorVersionNum.0\x64"
$destinationARM64 = "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\bin\$majorVersionNum.0\ARM64"

Copy-File -sourcePath $makecertx86 -destinationPath $destinationx86 -fileName "MakeCert.exe"
Copy-File -sourcePath $makecertX64 -destinationPath $destinationx64 -fileName "MakeCert.exe"
Copy-File -sourcePath $makecertARM64 -destinationPath $destinationARM64 -fileName "MakeCert.exe"

function Copy-Folder {
    param (
        [string]$sourceFolder,
        [string]$destinationFolder
    )

    if (Test-Path $sourceFolder) {
        Copy-Item -Path $sourceFolder -Destination $destinationFolder -Recurse -Force
        Write-Host "Copied $sourceFolder to $destinationFolder"
    } else {
        Write-Error "Source folder $sourceFolder not found"
    }
}

# Copying km, um, kmdf, umdf ARM64 headers to x64 folders
Copy-Folder -sourceFolder "C:\WDK\Microsoft.Windows.WDK.ARM64.$wdkVersion\c\Lib\$majorVersionNum.0\km\arm64" -destinationFolder "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\Lib\$majorVersionNum.0\km"
Copy-Folder -sourceFolder "C:\WDK\Microsoft.Windows.WDK.ARM64.$wdkVersion\c\Lib\$majorVersionNum.0\um\arm64" -destinationFolder "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\Lib\$majorVersionNum.0\um"
Copy-Folder -sourceFolder "C:\WDK\Microsoft.Windows.WDK.ARM64.$wdkVersion\c\Lib\wdf\kmdf\ARM64" -destinationFolder "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\Lib\wdf\kmdf"
Copy-Folder -sourceFolder "C:\WDK\Microsoft.Windows.WDK.ARM64.$wdkVersion\c\Lib\wdf\umdf\ARM64" -destinationFolder "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c\Lib\wdf\umdf"

# Set WDKContentRoot environment variable
$NugetWdkContentRoot = "C:\WDK\Microsoft.Windows.WDK.x64.$wdkVersion\c"
Write-Output "WDKContentRoot=$NugetWdkContentRoot" >> $env:GITHUB_ENV   