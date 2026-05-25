@echo off
setlocal EnableDelayedExpansion

set "SRC=kernels\src\hvector_add.cu"
set "RUN_DIR=kernels\runs\hvector-add-cuda"
set "EXE=%RUN_DIR%\hvector_add.exe"

if not exist "%RUN_DIR%" mkdir "%RUN_DIR%"
where nvcc >nul 2>nul
if errorlevel 1 (
  echo nvcc is required for hvector_add_cuda_v1 1>&2
  exit /b 127
)

where cl >nul 2>nul
if errorlevel 1 (
  set "VSWHERE=C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
  if not exist "!VSWHERE!" (
    echo cl.exe is required for nvcc and vswhere.exe was not found 1>&2
    exit /b 127
  )
  for /f "usebackq delims=" %%I in (`"!VSWHERE!" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSROOT=%%I"
  if not defined VSROOT (
    echo Visual Studio C++ build tools are required for nvcc 1>&2
    exit /b 127
  )
  call "!VSROOT!\VC\Auxiliary\Build\vcvars64.bat" >nul
  if errorlevel 1 exit /b %errorlevel%
)

nvcc -O2 --std=c++17 -o "%EXE%" "%SRC%" 1>nul
if errorlevel 1 exit /b %errorlevel%

"%EXE%"
