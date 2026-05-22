@echo off
rem
rem Apollia OS launcher — Windows.
rem
rem Ce script est inclus dans chaque archive de release (windows-*). Il garantit
rem que apollia-os.exe trouve l'interprète Python 3.13 bundlé même si l'user n'a
rem pas Python installé.
rem
rem Placement attendu :
rem   apollia-os\
rem   ├── apollia-os.bat       ← ce launcher
rem   ├── apollia-os.exe       ← le binaire (CRT statiquement lié)
rem   ├── python.exe           ← interprète bundlé (à la racine, layout Windows)
rem   ├── python313.dll
rem   └── Lib\, DLLs\
rem
rem Le binaire apollia-os.exe a son CRT (vcruntime140.dll, msvcp140.dll)
rem statiquement lié via +crt-static — aucun "Visual C++ Redist" requis.
rem
rem Usage :
rem   apollia-os.bat start
rem   apollia-os.bat run <agent> "prompt"
setlocal

set "HERE=%~dp0"
rem Strip trailing backslash.
if "%HERE:~-1%"=="\" set "HERE=%HERE:~0,-1%"

rem 1. PYO3_PYTHON pointe vers l'interprète bundlé.
set "PYO3_PYTHON=%HERE%\python.exe"

if not exist "%PYO3_PYTHON%" (
    echo error: bundled Python missing at %PYO3_PYTHON% 1>&2
    echo        l'archive est incomplete - re-telecharger depuis apollia.fr/download 1>&2
    exit /b 1
)

rem 2. Ajoute le dossier python\ au PATH pour que apollia-os.exe trouve
rem    python313.dll quand PyO3 dlopen() la libpython.
set "PATH=%HERE%;%PATH%"

rem 3. APOLLIA_PYTHON_BUNDLE_DIR pour les venvs par agent.
set "APOLLIA_PYTHON_BUNDLE_DIR=%HERE%"

rem 4. Lance le binaire.
"%HERE%\apollia-os.exe" %*
exit /b %ERRORLEVEL%
