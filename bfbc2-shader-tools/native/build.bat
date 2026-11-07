@echo off
rem Builds shaderview.exe with the Visual Studio C++ tools (any recent version with the Windows SDK).
setlocal
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VCVARS="
if exist "%VSWHERE%" for /f "usebackq delims=" %%i in (`call "%VSWHERE%" -latest -products * -find VC\Auxiliary\Build\vcvars64.bat`) do set "VCVARS=%%i"
if not defined VCVARS (echo Visual Studio C++ tools not found & exit /b 1)
call "%VCVARS%" >nul
cd /d "%~dp0"
cl /nologo /O2 /EHsc /std:c++17 shaderview.cpp /Fe:shaderview.exe /link d3d11.lib
