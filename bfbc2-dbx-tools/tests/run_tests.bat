@echo off
rem DbxEditGUI test suite: builds DbxEditGUI and the tests (DbxTests.cs with DbxEdit.cs and DbxEditGUI.cs), then runs
rem them. DbxEdit's command line is the suite's Rust core now (core\tests\test_dbxedit.py tests it).
rem   run_tests.bat                          unit tests of the DBX classes and the window (no game data needed)
rem   run_tests.bat <folder> [...]           + every binary .dbx under the folders: round trip, XML import identity,
rem                                          edited imports, imports without a reference
rem Set DBX_TESTS_ARGS for more options (e.g. --edit-every 5, --no-gui).
setlocal
set HERE=%~dp0
set CSC=C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe
rem DBX_SKIP_BUILD (set by the suite's run_tests.py, which builds DbxEditGUI only when its sources changed): the built
rem DbxEditGUI as it is, unless there is none
if defined DBX_SKIP_BUILD if exist "%HERE%..\DbxEditGUI.exe" goto built
call "%HERE%..\build.bat" >nul || (echo build failed & exit /b 1)
:built
"%CSC%" -nologo -optimize -main:DbxTests -out:"%HERE%DbxTests.exe" -r:System.Windows.Forms.dll -r:System.Drawing.dll ^
    "%HERE%DbxTests.cs" "%HERE%..\DbxEdit.cs" "%HERE%..\DbxEditGUI.cs" || (echo test build failed & exit /b 1)
if "%~1"=="" (
    "%HERE%DbxTests.exe" %DBX_TESTS_ARGS%
) else (
    "%HERE%DbxTests.exe" --corpus %* %DBX_TESTS_ARGS%
)
exit /b %errorlevel%
