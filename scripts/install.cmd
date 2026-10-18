@echo off
rem Installs Poros on Windows from its GitHub releases: the latest release, or the version given
rem as an argument. Downloads the setup program with curl, checks it against the release's
rem SHA256SUMS.txt with certutil and runs it silently, like install.ps1. Needs Windows 10 1803 or
rem later, which ship curl.exe. This file keeps CRLF line endings: cmd misreads labels in batch
rem files with bare LF.
rem
rem   curl -fsSL https://raw.githubusercontent.com/khaodius/poros/main/scripts/install.cmd -o install.cmd && install.cmd && del install.cmd

setlocal EnableExtensions DisableDelayedExpansion
rem Variables that may be read before this script sets them, cleared of any in the environment.
set "workdir="
set "version="

set "releases=https://github.com/khaodius/poros/releases"
set "suffix=_x64-setup.exe"
set "uninstall_key=HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Poros"

rem The version argument without its leading v. A substring of an undefined variable would break
rem the line, so a for loop strips it.
set "requested_version="
for /f "tokens=* delims=v" %%V in ("%~1") do set "requested_version=%%V"

if /i "%PROCESSOR_ARCHITECTURE%"=="x86" if not defined PROCESSOR_ARCHITEW6432 (
  set "message=Poros needs 64-bit Windows"
  goto fail
)

set "workdir=%TEMP%\poros-install-%RANDOM%%RANDOM%"
mkdir "%workdir%" 2>nul
if not exist "%workdir%\" (
  set "message=could not create %workdir%"
  goto fail
)

if defined requested_version (
  set "checksums_url=%releases%/download/v%requested_version%/SHA256SUMS.txt"
) else (
  set "checksums_url=%releases%/latest/download/SHA256SUMS.txt"
)
curl -fsSL --retry 3 -o "%workdir%\SHA256SUMS.txt" "%checksums_url%"
if errorlevel 1 (
  set "message=could not download %checksums_url%"
  goto fail
)

rem Only the setup program's line ends in the suffix. findstr cannot anchor to the end of a line
rem without a CR, so it matches anywhere in the line.
set "expected_hash="
set "file_name="
for /f "usebackq tokens=1,2" %%A in (`findstr /l /c:"%suffix%" "%workdir%\SHA256SUMS.txt"`) do (
  set "expected_hash=%%A"
  set "file_name=%%B"
)
if not defined file_name (
  set "message=the release has no Poros_*%suffix% setup program"
  goto fail
)
for /f "tokens=2 delims=_" %%V in ("%file_name%") do set "version=%%V"

echo Downloading Poros %version% (%file_name%)
set "setup=%workdir%\%file_name%"
curl -fSL --retry 3 --progress-bar -o "%setup%" "%releases%/download/v%version%/%file_name%"
if errorlevel 1 (
  set "message=could not download %file_name%"
  goto fail
)

rem certutil prints a heading line, then the hash; older versions put spaces between its bytes.
set "actual_hash="
for /f "skip=1 delims=" %%H in ('certutil -hashfile "%setup%" SHA256') do (
  if not defined actual_hash set "actual_hash=%%H"
)
if not defined actual_hash (
  set "message=could not compute the SHA-256 checksum of %file_name%"
  goto fail
)
set "actual_hash=%actual_hash: =%"
if /i not "%actual_hash%"=="%expected_hash%" (
  set "message=%file_name% does not match its SHA-256 checksum in SHA256SUMS.txt, so nothing was installed"
  goto fail
)

echo Installing Poros %version%
start "" /wait "%setup%" /S
set "setup_exit_code=%errorlevel%"
if not "%setup_exit_code%"=="0" (
  set "message=the setup program stopped with exit code %setup_exit_code%"
  goto fail
)

rem reg query prints each value as: name, type, data.
set "installed_version="
set "install_location=%LOCALAPPDATA%\Poros"
for /f "tokens=3" %%V in ('reg query "%uninstall_key%" /v DisplayVersion 2^>nul ^| findstr /l /c:"REG_SZ"') do (
  set "installed_version=%%V"
)
for /f "tokens=2,*" %%A in ('reg query "%uninstall_key%" /v InstallLocation 2^>nul ^| findstr /l /c:"REG_SZ"') do (
  set "install_location=%%~B"
)
if not "%installed_version%"=="%version%" (
  set "message=the setup program finished, but Windows does not list Poros %version% as installed"
  goto fail
)

rmdir /s /q "%workdir%" 2>nul
echo Poros %version% is installed in %install_location%. Open it from the Start menu or the desktop shortcut.
endlocal
exit /b 0

:fail
>&2 echo error: %message%
if defined workdir rmdir /s /q "%workdir%" 2>nul
endlocal
exit /b 1
