@echo off
rem Builds DbxEditGUI.exe (the window) next to this file with the .NET Framework 4 C# compiler that ships with
rem Windows: DbxEditGUI.cs and the DBX classes it uses (DbxEdit.cs). DbxEdit's command line is dbxedit.py (the suite's
rem Rust core); DbxEdit.exe was retired 2026-11-18.
setlocal
set HERE=%~dp0
set CSC=C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe
if not exist "%CSC%" (echo C# compiler not found: %CSC% & exit /b 1)
"%CSC%" -nologo -optimize -target:winexe -main:Bfbc2Dbx.Gui.GuiProgram -out:"%HERE%DbxEditGUI.exe" ^
    -r:System.Windows.Forms.dll -r:System.Drawing.dll "%HERE%DbxEditGUI.cs" "%HERE%DbxEdit.cs" || exit /b 1
echo Built DbxEditGUI.exe
