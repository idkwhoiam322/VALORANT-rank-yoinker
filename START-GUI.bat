@echo off
:: Change directory to batch script path
cd /d "%~dp0"
python "custom\gui\desktop_app.py"
pause
