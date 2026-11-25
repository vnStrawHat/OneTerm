"""Summarize a folded-stack file from sampler.cs (root;...;leaf count)."""
import re
import sys
from collections import Counter

path = sys.argv[1]
stacks = []
for line in open(path, encoding="utf-8"):
    line = line.rstrip("\n")
    if not line:
        continue
    frames, n = line.rsplit(" ", 1)
    stacks.append((frames.split(";"), int(n)))
total = sum(n for _, n in stacks)


def share(pred):
    return sum(n for f, n in stacks if pred(f)) / total


def has(sub):
    return lambda f: any(sub in x for x in f)


print(f"busy samples: {total}")
cats = [
    ("Window::draw (build the frame)", has("gpui::window::Window::draw")),
    ("  draw_roots", has("Window::draw_roots")),
    ("    taffy layout (compute_layout)", has("Window::compute_layout")),
    ("    Render::render of views (view render fns)", lambda f: any(re.search(r"(any_view::render<|view::impl\$4::render|RenderOnce>::render|impl\$\d+::render<)", x) for x in f)),
    ("    paint phase", lambda f: any("::paint<" in x or x.endswith("::paint") for x in f) and not any("prepaint" in x for x in f)),
    ("  arena clear / element drop", lambda f: any("Arena::force_clear" in x or "ArenaClearNeeded" in x for x in f)),
    ("present (DirectX)", has("DirectXRenderer")),
    ("WM_GETOBJECT (UI Automation)", has("handle_wm_getobject")),
    ("IME / text services (Ctf/TF_)", lambda f: any(x.startswith("Ctf") or x.startswith("TF_") for x in f)),
    ("heap alloc/free (RtlAllocateHeap/RtlFreeHeap anywhere)", lambda f: any(x in ("RtlAllocateHeap", "RtlFreeHeap", "RtlReAllocateHeap") for x in f[-4:])),
    ("PTY/terminal (oneterm_terminal_view element)", has("oneterm_terminal_view::render::element")),
]
for name, pred in cats:
    print(f"{share(pred):6.1%}  {name}")

# Which view subtrees the frame spends in: the outermost ViewElement<...> request_layout/prepaint/paint frames.
def view_of(f):
    views = [re.search(r"ViewElement<([^>]*(?:<[^>]*>)?[^>]*)>", x) for x in f]
    names = [m.group(1) for m in views if m]
    return names

by_view = Counter()
for f, n in stacks:
    names = view_of(f)
    for v in set(names):
        by_view[v] += n
print("\nView subtrees (inclusive, any phase):")
for v, n in by_view.most_common(25):
    print(f"{n / total:6.1%}  {v}")

leaf = Counter()
for f, n in stacks:
    leaf[f[-1]] += n
print("\nTop exclusive:")
for v, n in leaf.most_common(25):
    print(f"{n / total:6.1%}  {v[:140]}")
