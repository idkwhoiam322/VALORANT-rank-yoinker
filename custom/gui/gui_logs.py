"""
custom/gui/gui_logs.py -- logging for the desktop launcher
(custom/gui/desktop_app.py), kept separate from src/logs.py's log-N.txt
(main.py/backend logging).

 Writes logs\\vry_gui-N.txt using the same "pick the next free number, one
file per run" scheme and timestamp format as src/logs.py, so both kinds of
logs sit side by side in the same folder and look consistent.

Also exposes GuiLogger.tail(), used by desktop_app.py's pywebview js_api to
feed the loading page's live log viewer (see docs/vry_gui.html's #loadingOverlay).
"""

import glob
import os
import time


class GuiLogger:
    def __init__(self, project_root):
        self.logs_directory = os.path.join(project_root, "logs")
        self.log_file_path = None
        self._opened = False

    def _pick_log_file_path(self):
        if not os.path.exists(self.logs_directory):
            os.makedirs(self.logs_directory, exist_ok=True)

        existing = glob.glob(os.path.join(self.logs_directory, "vry_gui-*.txt"))
        numbers = []
        for file in existing:
            try:
                numbers.append(int(os.path.basename(file)[len("vry_gui-"):-len(".txt")]))
            except ValueError:
                continue
        if not numbers:
            numbers.append(0)

        next_number = max(numbers) + 1
        return os.path.join(self.logs_directory, f"vry_gui-{next_number}.txt")

    def log(self, log_string: str):
        try:
            if self.log_file_path is None:
                self.log_file_path = self._pick_log_file_path()

            current_time = time.strftime("%Y.%m.%d-%H.%M.%S", time.localtime(time.time()))
            with open(self.log_file_path, "a" if self._opened else "w", encoding="utf-8") as log_file:
                self._opened = True
                log_file.write(f"[{current_time}] {log_string}\n")
        except Exception:
            # GUI logging should never be the reason the app crashes.
            pass

    def tail(self, max_chars: int = 6000) -> str:
        """Read back everything logged so far (for the loading page). Kept
        deliberately simple/synchronous -- these log files are tiny."""
        if self.log_file_path is None or not os.path.exists(self.log_file_path):
            return ""
        try:
            with open(self.log_file_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except Exception:
            return ""
        return text[-max_chars:] if len(text) > max_chars else text
