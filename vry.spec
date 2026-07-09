# -*- mode: python ; coding: utf-8 -*-
#
# Builds vry.exe: a single launcher that shows the desktop UI and manages
# the vRY backend itself (see custom/gui/desktop_app.py for exactly how,
# including the --vry-backend / --config flags it uses internally).
#
# Usage:
#   pip install pyinstaller
#   pyinstaller vry.spec
#
# Output: dist/vry/vry.exe plus a dist/vry/_internal support folder.
# Ship the whole "vry" folder -- the only file the person needs to double
# click is vry.exe.
#
# This intentionally builds as one-folder (COLLECT), not one-file. A
# one-file exe has to self-extract to a temp folder on every launch, and
# since the GUI launches a second copy of itself as the backend process,
# one-file mode means paying that extraction cost twice per run. One-folder
# avoids that entirely while still only ever showing the person one .exe.
from pathlib import Path
from PyInstaller.utils.hooks import collect_submodules

ROOT = Path(SPEC).resolve().parent

hiddenimports = [
    'InquirerPy', 'pfzy', 'prompt_toolkit',
    'nest_asyncio', 'pypresence', 'requests', 'rich',
    'urllib3', 'websocket_server', 'websockets', 'webview',
    'src.account_manager', 'src.account_manager.account_manager',
    'src.account_manager.account_config', 'src.account_manager.account_auth',
]
hiddenimports += collect_submodules('src')
hiddenimports += collect_submodules('custom')

a = Analysis(
    [str(ROOT / 'custom' / 'gui' / 'desktop_app.py')],
    pathex=[str(ROOT)],
    binaries=[],
    datas=[
        (str(ROOT / 'config.json'), '.'),
        (str(ROOT / 'docs'), 'docs'),
        (str(ROOT / 'assets'), 'assets'),
        (str(ROOT / 'main.py'), '.'),
    ],
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        'tkinter', 'turtle', 'unittest', 'xmlrpc', 'ftplib', 'imaplib',
        'poplib', 'smtplib', 'telnetlib', 'nntplib', 'pdb', 'doctest',
        'distutils', 'lib2to3', 'test', 'turtledemo',
    ],
    noarchive=False,
    optimize=2,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='vry',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=str(ROOT / 'assets' / 'Logo.ico'),
)

coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='vry',
)
