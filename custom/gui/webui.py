"""
Helpers for the "Comprehensive Match Loadouts" page (docs/vry_gui.html).

    from custom.gui.webui import print_webui_link
    ...
    print_webui_link(PROJECT_ROOT)   # inside print_info()
"""

from pathlib import Path

from src.colors import color

WEBUI_RELATIVE_PATH = Path("docs/vry_gui.html")


def _resolve_page(project_root):
    return (project_root / WEBUI_RELATIVE_PATH).resolve()


def print_webui_link(project_root):
    """Print a clickable link, styled the same way as the existing
    'Player Inventories' link in main.py's print_info()."""
    url = _resolve_page(project_root).as_uri()
    link = f"\033]8;;{url}\033\\View in browser\033]8;;\033\\"
    print(
        "\nComprehensive Match Loadouts",
        color(f"- {link}", fore=(255, 127, 80)),
    )
