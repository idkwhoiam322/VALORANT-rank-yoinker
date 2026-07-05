import sys
from cx_Freeze import setup, Executable
from src.constants import version

# with open("requirements.txt", "r") as f:
#     requirements = f.read().splitlines()
#     packages = []
#     for r in requirements:
#         if "==" in r:
#             packages.append(r.split("==")[0])
#         elif "~=" in r:
#             packages.append(r.split("~=")[0])
#         elif ">=" in r:
#             packages.append(r.split(">=")[0])
#         elif "<=" in r:
#             packages.append(r.split("<=")[0])

build_exe_options = {
    "path": sys.path,
    "include_files": ['configurator.bat', 'updatescript.bat', 'config.json', 'docs'],
    "packages": ["requests", "InquirerPy", "websockets", "pypresence", "nest_asyncio", "rich", "websocket_server", "webview"],
    "excludes": ["tkinter", "test", "unittest", "pygments", "xmlrpc"]
}

setup(
    name = "VALORANT rank yoinker",
    version = version,
    description='vRY - VALORANT rank yoinker',
    executables = [
        Executable("main.py", icon="./assets/Logo.ico", target_name="vry.exe"),
        Executable("custom/gui/desktop_app.py", icon="./assets/Logo.ico", target_name="vry-gui.exe", base="Win32GUI" if sys.platform == "win32" else None),
    ],
    options={"build_exe": build_exe_options}
)
