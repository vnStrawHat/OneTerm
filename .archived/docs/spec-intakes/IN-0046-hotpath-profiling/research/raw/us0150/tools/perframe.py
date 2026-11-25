"""Sampler busy samples per drawn frame, by subtree (US-0150).

    python perframe.py <dir> smp-before-idle smp-v2-idle ...

Frames come from the hotpath report of the same run (`OneTermWorkspace::render` calls).
A predicate counts a stack once when any frame of it matches.
"""
import json
import os
import sys

d = sys.argv[1]
CATS = [
    ("all busy samples", lambda f: True),
    ("Window::draw", lambda f: any("gpui::window::Window::draw" in x for x in f)),
    ("  title bar (AppTitleBar subtree)", lambda f: any("title_bar::AppTitleBar" in x for x in f)),
    ("    of which the toggle group", lambda f: any("toggle::ToggleGroup" in x for x in f)),
    ("    of which the kit's window controls", lambda f: any("title_bar::WindowControls" in x for x in f)),
    ("  status bar (bar, labels, measuring)", lambda f: any(("status_bar::StatusBar" in x) or ("statusbar::" in x) or ("measure_status_text" in x) for x in f)),
    ("  dock area", lambda f: any("dock_area::DockArea" in x for x in f)),
    ("    tab bars", lambda f: any("tab_bar::TabBar" in x for x in f)),
    ("    dropdown triggers (+ and ...)", lambda f: any("DropdownMenuPopover" in x for x in f)),
    ("  taffy layout", lambda f: any("Window::compute_layout" in x for x in f)),
    ("present (DirectX)", lambda f: any("DirectXRenderer" in x for x in f)),
    ("UI Automation (WM_GETOBJECT)", lambda f: any("handle_wm_getobject" in x for x in f)),
    ("IME / text services", lambda f: any(x.startswith("Ctf") or x.startswith("TF_") for x in f)),
]
runs = sys.argv[2:]
res = {}
for run in runs:
    frames = next(x["calls"] for x in json.load(open(os.path.join(d, run + ".json"), encoding="utf-8"))["functions_timing"]["data"]
                  if x["name"].endswith("OneTermWorkspace::render"))
    counts = [0] * len(CATS)
    for line in open(os.path.join(d, run + ".folded"), encoding="utf-8"):
        frames_s, n = line.rstrip("\n").rsplit(" ", 1)
        f = frames_s.split(";")
        n = int(n)
        for i, (_, pred) in enumerate(CATS):
            if pred(f):
                counts[i] += n
    res[run] = (frames, counts)
print("| Samples per frame | " + " | ".join(runs) + " |")
print("| --- |" + " ---: |" * len(runs))
print("| frames drawn | " + " | ".join(str(res[r][0]) for r in runs) + " |")
for i, (name, _) in enumerate(CATS):
    print(f"| {name.strip()} | " + " | ".join(f"{res[r][1][i] / res[r][0]:.1f}" for r in runs) + " |")
