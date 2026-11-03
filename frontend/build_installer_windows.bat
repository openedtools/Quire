@echo off
rem Builds a shareable Windows installer (.exe) of this fork. CPU build, not code-signed.
rem Unlike clean_build_windows.bat this keeps node_modules, so repeat builds are much faster.
cd /d "%~dp0"

rem The speech engine compiles C++ code and needs CMake.
where cmake >nul 2>nul
if errorlevel 1 (
  echo CMake is not installed. Run:  winget install Kitware.CMake
  echo Then close this window, open a new Command Prompt, and run this script again.
  goto fail
)

rem Portable-build settings, copied from the project's own distribution builds. Without them the speech
rem engine is tuned to THIS computer's CPU and can crash ("illegal instruction") on a friend's computer.
set "HOOK=%~dp0..\.github\force-portable-ggml.cmake"
if not exist "%HOOK%" (
  echo Could not find %HOOK%
  goto fail
)
for %%I in ("%HOOK%") do set "HOOK=%%~fI"
set "CMAKE_PROJECT_INCLUDE=%HOOK:\=/%"
set "RUSTFLAGS=-C target-cpu=x86-64-v2"

echo Installing dependencies...
call pnpm install --frozen-lockfile
if errorlevel 1 goto fail

echo.
echo Building the llama-helper helper program (the installer bundles it and refuses to build without it)...
if not exist "%~dp0..\llama-helper" (
  echo Could not find the llama-helper folder next to this one.
  goto fail
)
pushd "%~dp0..\llama-helper"
call cargo build --release
if errorlevel 1 (
  popd
  goto fail
)
popd
for /f "tokens=2" %%i in ('rustc -vV ^| findstr "host:"') do set TARGET_TRIPLE=%%i
if not exist "%~dp0src-tauri\binaries" mkdir "%~dp0src-tauri\binaries"
del /q "%~dp0src-tauri\binaries\llama-helper*" 2>nul
set "HELPER_SRC=%~dp0..\target\release\llama-helper.exe"
if not exist "%HELPER_SRC%" set "HELPER_SRC=%~dp0..\llama-helper\target\release\llama-helper.exe"
if not exist "%HELPER_SRC%" (
  echo Could not find the built llama-helper.exe.
  goto fail
)
copy /Y "%HELPER_SRC%" "%~dp0src-tauri\binaries\llama-helper-%TARGET_TRIPLE%.exe" >nul
echo llama-helper ready for %TARGET_TRIPLE%.

echo.
echo Building the installer. The first build takes a long time (often 20-40 minutes).
call pnpm tauri build --bundles nsis
if errorlevel 1 goto fail

echo.
echo Done. Your installer is below, with its SHA-256 fingerprint.
echo Send the fingerprint to your friend separately (a different message), so they can check the file.
for %%D in ("%~dp0..\target\release\bundle\nsis" "%~dp0src-tauri\target\release\bundle\nsis") do (
  if exist "%%~D\*.exe" for %%F in ("%%~D\*.exe") do (
    echo   %%~fF
    certutil -hashfile "%%~fF" SHA256
  )
)
goto :eof

:fail
echo.
echo The build failed. Scroll up to the FIRST line that says "error" to see why.
exit /b 1
