"""Prints the AT-SPI tree of the running slint-spike window.

Run with the system python that has pyatspi: python3.12 -I a11y_dump.py
"""
import sys

import pyatspi

MAX_CHILDREN = 8


def describe(acc):
    states = acc.getState()
    flags = [
        name
        for name, st in (
            ("focused", pyatspi.STATE_FOCUSED),
            ("focusable", pyatspi.STATE_FOCUSABLE),
            ("selected", pyatspi.STATE_SELECTED),
            ("selectable", pyatspi.STATE_SELECTABLE),
            ("checked", pyatspi.STATE_CHECKED),
            ("enabled", pyatspi.STATE_ENABLED),
        )
        if states.contains(st)
    ]
    attrs = {}
    try:
        for kv in acc.getAttributes():
            k, _, v = kv.partition(":")
            if k in ("posinset", "setsize", "level"):
                attrs[k] = v
    except Exception:
        pass
    return f"{acc.getRoleName()} {acc.name!r} [{', '.join(flags)}] {attrs if attrs else ''}"


def walk(acc, depth=0):
    print("  " * depth + describe(acc))
    n = acc.childCount
    limit = MAX_CHILDREN if acc.getRoleName() in ("list box", "list") else 100
    for i in range(n):
        if i == limit and n > limit + 2:
            print("  " * (depth + 1) + f"... {n - limit - 1} more ...")
            walk(acc.getChildAtIndex(n - 1), depth + 1)
            break
        walk(acc.getChildAtIndex(i), depth + 1)


desktop = pyatspi.Registry.getDesktop(0)
apps = [desktop.getChildAtIndex(i) for i in range(desktop.childCount)]
print("applications:", [a.name for a in apps])
for app in apps:
    if app.name and "slint" in app.name.lower() or any(
        "PDF Converter" in (app.getChildAtIndex(i).name or "") for i in range(app.childCount)
    ):
        walk(app)
        focus = [None]

        def find_focus(acc):
            if acc.getState().contains(pyatspi.STATE_FOCUSED):
                focus[0] = acc
            for i in range(acc.childCount):
                find_focus(acc.getChildAtIndex(i))

        find_focus(app)
        print("focused:", describe(focus[0]) if focus[0] else None)
        if focus[0]:
            print("  attributes:", list(focus[0].getAttributes()))
            try:
                sel = focus[0].parent.querySelection()
                print("  list selection count:", sel.nSelectedChildren)
            except NotImplementedError:
                print("  list box has no Selection interface")
        sys.exit(0)
print("slint-spike not found")
sys.exit(1)
