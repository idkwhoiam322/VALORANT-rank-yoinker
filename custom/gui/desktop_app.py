"""
custom/gui/desktop_app.py -- single entry point that runs vRY as a desktop app
instead of "cmd window + browser tab".

This one file is built into the single vry.exe (see vry.spec) and is also
what START-GUI.bat runs directly during development. It picks one of three
roles based on a hidden command-line flag:

    (no flags)      -- GUI mode (the normal case). Opens the pywebview
                        window immediately, and -- unless something is
                        already listening on the configured port -- launches
                        a second copy of this same executable in
                        "--vry-backend" mode to do the actual data fetching.
    --vry-backend   -- Backend mode. Runs main.py in this process and exits
                        when it exits. No window. This has to be a real
                        separate process rather than a background thread:
                        the "Force Refresh" button in the web UI tells the
                        backend to restart itself via os.execv() (see
                        src/server.py), which replaces the entire calling
                        process -- fine for a dedicated backend process,
                        fatal if it were sharing a process with the GUI
                        window.
    --config        -- Runs main.py's --config wizard in a visible console.

On Windows, os.execv() does NOT keep the same PID the way it does on POSIX --
Python emulates it by spawning a brand new process and only then exiting the
original one. That means the subprocess.Popen handle this file holds for the
backend can go stale the moment "Force Refresh" is used: the original PID
exits (so .terminate() on it is a no-op), while the *real* running backend is
a different, untracked PID that never gets cleaned up when the GUI closes.
To make cleanup correct regardless of how many times the backend has
restarted itself, the backend process is placed in a Windows Job Object with
JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE (see _create_job_object_with_kill_on_close
below). Any process the backend spawns -- including a self-restart via
os.execv -- automatically joins the same job, and closing the job's handle
force-kills everything still in it in one shot.

Usage (from the project root):
    python custom/gui/desktop_app.py
    (or double-click START-GUI.bat)

Packaging as vry.exe: `pip install pyinstaller` then `pyinstaller vry.spec`
(see vry.spec -- it builds this file into dist/vry/vry.exe).
"""

import ctypes
import json
import os
import runpy
import socket
import subprocess
import sys
from pathlib import Path

try:
    # vry.spec includes "custom" as hiddenimports, so the frozen build's
    # importer resolves this dotted path fine.
    from custom.gui.gui_logs import GuiLogger
except ImportError:
    # Dev mode (`python custom/gui/desktop_app.py` directly): the project
    # root isn't on sys.path yet, so "custom.gui.gui_logs" isn't importable
    # as a package. Import the sibling file directly instead.
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from gui_logs import GuiLogger

def _get_project_root():
    """Directory that holds config.json / docs / assets / main.py.

    In development that's the repo root, three levels up from this file
    (custom/gui/desktop_app.py).

    In a frozen build, PyInstaller's bootloader only preserves the
    *basename* of the entry-point script, not its original subfolder path,
    so __file__ can't be used to derive this reliably. sys._MEIPASS always
    points at the folder vry.spec's `datas` were collected into, so use
    that instead when frozen.
    """
    if getattr(sys, "frozen", False):
        return Path(sys._MEIPASS)
    return Path(__file__).parent.parent.parent.resolve()


PROJECT_ROOT = _get_project_root()
MAIN_PY = PROJECT_ROOT / "main.py"
VRY_GUI_HTML = PROJECT_ROOT / "docs" / "vry_gui.html"

BACKEND_FLAG = "--vry-backend"
CONFIG_FLAG = "--config"

FROZEN = getattr(sys, "frozen", False)

        # GUI-side log (logs\vry_gui-N.txt) -- separate from src/logs.py's
# log-N.txt (backend business logic) and logs\backend.log (raw stdout
# redirect below). Also feeds the loading page's live log viewer.
gui_log = GuiLogger(PROJECT_ROOT)


# ---------------------------------------------------------------------------
# Windows Job Object: ensures the backend (and anything IT spawns, including
# a self-restart via os.execv in src/server.py) always gets cleaned up when
# the GUI closes, no matter how many times "Force Refresh" has replaced the
# original PID. See the module docstring above for the full "why".
# ---------------------------------------------------------------------------

_JobObjectExtendedLimitInformation = 9  # JobObjectExtendedLimitInformation
_JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000


class _IO_COUNTERS(ctypes.Structure):
    _fields_ = [
        ("ReadOperationCount", ctypes.c_ulonglong),
        ("WriteOperationCount", ctypes.c_ulonglong),
        ("OtherOperationCount", ctypes.c_ulonglong),
        ("ReadTransferCount", ctypes.c_ulonglong),
        ("WriteTransferCount", ctypes.c_ulonglong),
        ("OtherTransferCount", ctypes.c_ulonglong),
    ]


class _JOBOBJECT_BASIC_LIMIT_INFORMATION(ctypes.Structure):
    _fields_ = [
        ("PerProcessUserTimeLimit", ctypes.c_int64),
        ("PerJobUserTimeLimit", ctypes.c_int64),
        ("LimitFlags", ctypes.c_uint32),
        ("MinimumWorkingSetSize", ctypes.c_size_t),
        ("MaximumWorkingSetSize", ctypes.c_size_t),
        ("ActiveProcessLimit", ctypes.c_uint32),
        ("Affinity", ctypes.c_size_t),
        ("PriorityClass", ctypes.c_uint32),
        ("SchedulingClass", ctypes.c_uint32),
    ]


class _JOBOBJECT_EXTENDED_LIMIT_INFORMATION(ctypes.Structure):
    _fields_ = [
        ("BasicLimitInformation", _JOBOBJECT_BASIC_LIMIT_INFORMATION),
        ("IoInfo", _IO_COUNTERS),
        ("ProcessMemoryLimit", ctypes.c_size_t),
        ("JobMemoryLimit", ctypes.c_size_t),
        ("PeakProcessMemoryUsed", ctypes.c_size_t),
        ("PeakJobMemoryUsed", ctypes.c_size_t),
    ]


def _create_job_object_with_kill_on_close():
    """Create a Windows Job Object that force-kills every process still
    inside it -- including any it spawned itself after we last touched it --
    the moment its handle is closed.

    Returns the job handle, or None on non-Windows platforms or if anything
    here fails. None is always a safe return: callers just get less of a
    safety net, nothing breaks."""
    if os.name != "nt":
        return None
    try:
        kernel32 = ctypes.windll.kernel32
        job = kernel32.CreateJobObjectW(None, None)
        if not job:
            return None

        info = _JOBOBJECT_EXTENDED_LIMIT_INFORMATION()
        info.BasicLimitInformation.LimitFlags = _JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        ok = kernel32.SetInformationJobObject(
            job,
            _JobObjectExtendedLimitInformation,
            ctypes.byref(info),
            ctypes.sizeof(info),
        )
        if not ok:
            kernel32.CloseHandle(job)
            return None
        return job
    except Exception as e:
        gui_log.log(f"could not create job object (non-fatal): {e!r}")
        return None


def _assign_process_to_job(job, process):
    """Put `process` (a subprocess.Popen) into `job`. Any process it spawns
    afterwards -- e.g. its own os.execv restart -- automatically joins the
    same job too; nothing further needs to be tracked or reassigned."""
    if job is None:
        return
    try:
        kernel32 = ctypes.windll.kernel32
        # Popen._handle is the raw Win32 process handle. It's an
        # implementation detail, but a long-stable one on CPython/Windows,
        # and the simplest way to get a job-assignable handle without
        # re-opening the process by PID.
        if not kernel32.AssignProcessToJobObject(job, int(process._handle)):
            gui_log.log("AssignProcessToJobObject failed (non-fatal)")
    except Exception as e:
        gui_log.log(f"could not assign backend to job object (non-fatal): {e!r}")


def _close_job_object(job):
    if job is None:
        return
    try:
        ctypes.windll.kernel32.CloseHandle(job)
    except Exception:
        pass


def get_configured_port():
    try:
        with open(PROJECT_ROOT / "config.json", "r") as f:
            return int(json.load(f).get("port", 1100))
    except Exception:
        return 1100


def port_is_open(port, host="127.0.0.1", timeout=0.5):
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except OSError:
        return False


def _hide_own_console_if_present_none():
    """On Windows, give this process a real (but invisible) console if it
    doesn't already have one.

    main.py calls os.system("cls") / os.system("title ...") in a few spots.
    A process launched with CREATE_NO_WINDOW has *no* console at all, so
    each of those calls has to spin up a brand new console to run in -- and
    that new console flashes on screen for a split second before closing.
    Pre-allocating a hidden console means those os.system() calls reuse it
    instead, so nothing ever flashes.
    """
    if os.name != "nt":
        return
    kernel32 = ctypes.windll.kernel32
    if kernel32.GetConsoleWindow():
        return  # already attached to one (e.g. running from a terminal)
    if kernel32.AllocConsole():
        hwnd = kernel32.GetConsoleWindow()
        if hwnd:
            SW_HIDE = 0
            ctypes.windll.user32.ShowWindow(hwnd, SW_HIDE)


def _attach_visible_console_io():
    """Used for --config: allocate (or reuse) a *visible* console and point
    stdio at it, since the config wizard needs real keyboard input."""
    if os.name != "nt":
        return
    kernel32 = ctypes.windll.kernel32
    if not kernel32.GetConsoleWindow():
        kernel32.AllocConsole()
    sys.stdout = open("CONOUT$", "w", encoding="utf-8", errors="replace")
    sys.stderr = open("CONOUT$", "w", encoding="utf-8", errors="replace")
    sys.stdin = open("CONIN$", "r", encoding="utf-8", errors="replace")


def _redirect_stdio_to_log():
    """A frozen windowed executable has no usable stdout/stderr, which
    crashes the print()/rich-console calls throughout main.py. Send them to
    a log file instead."""
    log_path = PROJECT_ROOT / "logs" / "backend.log"
    log_path.parent.mkdir(exist_ok=True)
    log_file = open(log_path, "a", encoding="utf-8", errors="ignore")
    sys.stdout = log_file
    sys.stderr = log_file


def run_backend_in_this_process():
    """Entry point when (re-)launched with --vry-backend: run main.py's
    top-level code directly."""
    _hide_own_console_if_present_none()
    if FROZEN:
        _redirect_stdio_to_log()
    os.chdir(PROJECT_ROOT)
    runpy.run_path(str(MAIN_PY), run_name="__main__")


def run_configurator():
    """Entry point for `vry.exe --config` / `python desktop_app.py --config`:
    runs main.py's config wizard in a visible console."""
    _attach_visible_console_io()
    os.chdir(PROJECT_ROOT)
    runpy.run_path(str(MAIN_PY), run_name="__main__")


def _backend_launch_command():
    """Command that starts a fresh backend process: re-launch *this same
    program* with a hidden flag, instead of depending on a separate vry.exe
    that may or may not exist next to us. Works identically for the frozen
    exe and for `python custom/gui/desktop_app.py` during development."""
    if FROZEN:
        return [sys.executable, BACKEND_FLAG]
    return [sys.executable, str(Path(__file__).resolve()), BACKEND_FLAG]


def start_backend_process(job=None):
    creationflags = 0
    startupinfo = None
    if os.name == "nt":
        creationflags = subprocess.CREATE_NO_WINDOW
        startupinfo = subprocess.STARTUPINFO()
        startupinfo.dwFlags |= subprocess.STARTF_USESHOWWINDOW

    env = os.environ.copy()
    env.setdefault("PYTHONIOENCODING", "utf-8")
    env.setdefault("PYTHONUTF8", "1")

    gui_log.log(f"starting backend process: {_backend_launch_command()}")
    process = subprocess.Popen(
        _backend_launch_command(),
        cwd=str(PROJECT_ROOT),
        stdin=subprocess.DEVNULL,
        creationflags=creationflags,
        startupinfo=startupinfo,
        env=env,
    )
    gui_log.log(f"backend process started, pid={process.pid}")
    _assign_process_to_job(job, process)
    return process


class Api:
    """Bound to the pywebview window as js_api. Exposed in JS as
    window.pywebview.api.<method>(...), each call returning a Promise.
    Used by docs/vry_gui.html's loading overlay to show gui_logs-N.txt live
    while the backend/websocket isn't up yet."""

    def get_gui_log_tail(self):
        return gui_log.tail()


def run_gui():
    import webview  # deferred: the backend-only process never needs this

    port = get_configured_port()
    gui_log.log(f"desktop app starting, configured port={port}")

    backend_job = _create_job_object_with_kill_on_close()
    gui_log.log("backend job object created" if backend_job else "backend job object unavailable (non-Windows, or creation failed)")

    backend_already_running = port_is_open(port)
    backend_process = None
    if backend_already_running:
        gui_log.log(f"port {port} already open, assuming a backend is already running")
    else:
        backend_process = start_backend_process(job=backend_job)

    # Show the window right away instead of blocking here until the backend
    # port opens. docs/vry_gui.html already reconnects its websocket
    # automatically every 2.5s on its own, so waiting first only adds to
    # startup time without helping anything. The loading overlay (driven by
    # Api.get_gui_log_tail) covers the gap in the meantime.
    url = VRY_GUI_HTML.resolve().as_uri() + f"#port={port}"
    webview.create_window(
        "VALORANT rank yoinker",
        url,
        js_api=Api(),
        width=1500,
        height=950,
        min_size=(900, 600),
        maximized=True,
    )

    try:
        webview.start()
    finally:
        gui_log.log("webview closed, shutting down")
        if backend_process is not None:
            backend_process.terminate()
            try:
                backend_process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                backend_process.kill()
            gui_log.log("backend process stopped")
        # Belt and suspenders: terminate() above only ever reaches the
        # *original* backend PID. If Force Refresh replaced it in the
        # meantime (see the module docstring), the real running backend is
        # a different, untracked process -- closing the job handle here
        # kills it (and anything else still in the job) regardless.
        _close_job_object(backend_job)


def main():
    if BACKEND_FLAG in sys.argv:
        gui_log.log("entering backend mode (--vry-backend)")
        run_backend_in_this_process()
    elif CONFIG_FLAG in sys.argv:
        gui_log.log("entering configurator mode (--config)")
        run_configurator()
    else:
        try:
            run_gui()
        except Exception as e:
            # A windowed (console=False) build has nowhere to show an
            # uncaught exception, so log it before re-raising.
            gui_log.log(f"fatal error in run_gui: {e!r}")
            raise


if __name__ == "__main__":
    main()