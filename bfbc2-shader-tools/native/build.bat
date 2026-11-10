@echo off
rem Builds shaderview.exe with the Visual Studio C++ tools (any recent version with the Windows SDK).
rem   build.bat [output folder]      default: this folder (where Project OPFOR's previewers look for it);
rem                                  renderqa.py builds into the suite's build\ folder (gitignored)
setlocal
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VCVARS="
if exist "%VSWHERE%" for /f "usebackq delims=" %%i in (`call "%VSWHERE%" -latest -products * -find VC\Auxiliary\Build\vcvars64.bat`) do set "VCVARS=%%i"
if not defined VCVARS (echo Visual Studio C++ tools not found & exit /b 1)
set "OUT=%~1"
if not defined OUT set "OUT=%~dp0"
if not exist "%OUT%" mkdir "%OUT%"
call "%VCVARS%" >nul
cd /d "%OUT%"
cl /nologo /O2 /EHsc /std:c++17 "%~dp0shaderview.cpp" /Fe:shaderview.exe /link d3d11.lib
