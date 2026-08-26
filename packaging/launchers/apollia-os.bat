@echo off
rem
rem Apollia OS launcher - Windows.
rem
rem This script ships in every release archive (windows-*). It guarantees that
rem apollia-os.exe finds the bundled Python 3.13 interpreter even when the user
rem has no Python installed.
rem
rem Expected layout:
rem   apollia-os\
rem   |-- apollia-os.bat       <- this launcher
rem   |-- apollia-os.exe       <- the binary (statically linked CRT)
rem   |-- python.exe           <- bundled interpreter (at the root, Windows layout)
rem   ├── python313.dll
rem   └── Lib\, DLLs\
rem
rem The apollia-os.exe binary carries its CRT (vcruntime140.dll, msvcp140.dll)
rem statically linked with +crt-static - no "Visual C++ Redist" required.
rem
rem Usage :
rem   apollia-os.bat start
rem   apollia-os.bat run <agent> "prompt"
setlocal

set "HERE=%~dp0"
rem Strip trailing backslash.
if "%HERE:~-1%"=="\" set "HERE=%HERE:~0,-1%"

rem 1. PYO3_PYTHON points at the bundled interpreter.
set "PYO3_PYTHON=%HERE%\python.exe"

if not exist "%PYO3_PYTHON%" (
    echo error: bundled Python missing at %PYO3_PYTHON% 1>&2
    echo        the archive is incomplete - download it again from 1>&2
    echo        https://github.com/Apollia-OS/apollia-os/releases 1>&2
    exit /b 1
)

rem 2. Adds the python\ directory to PATH so apollia-os.exe finds
rem    python313.dll when PyO3 dlopen()s libpython.
set "PATH=%HERE%;%PATH%"

rem 3. APOLLIA_PYTHON_BUNDLE_DIR, for the per-agent venvs.
set "APOLLIA_PYTHON_BUNDLE_DIR=%HERE%"

rem 4. Run the binary.
"%HERE%\apollia-os.exe" %*
exit /b %ERRORLEVEL%
