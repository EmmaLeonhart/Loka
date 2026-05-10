@echo off
echo Loading stress test data into Loka (port 3030)...
echo Make sure Loka is running first: !serve.bat
python "%~dp0stress_test.py"
pause
