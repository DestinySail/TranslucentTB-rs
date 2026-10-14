@echo off
"%~dp0clearbar.exe" --enable-startup
if errorlevel 1 exit /b 1
start "" "%~dp0clearbar.exe"
