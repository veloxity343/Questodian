# PyInstaller recipe for the desktop app. Build with `make desktop` (after `make ui`).
# -*- mode: python -*-
import sys

from PyInstaller.utils.hooks import collect_data_files, collect_submodules

datas = collect_data_files("questodian")  # built UI, SRD data, migration template
# Alembic imports env.py and the migration scripts from disk, so ship them as files.
datas += collect_data_files("questodian", subdir="migrations", include_py_files=True)
hiddenimports = collect_submodules("uvicorn") + collect_submodules("questodian") + ["webview"]

a = Analysis(
    ["entry.py"],
    pathex=[],
    datas=datas,
    hiddenimports=hiddenimports,
    excludes=["tkinter", "pytest"],
    noarchive=False,
)
pyz = PYZ(a.pure)
exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="Questodian",
    console=False,  # a window app; run `Questodian serve` from a terminal for the browser version
    disable_windowed_traceback=False,
)
coll = COLLECT(exe, a.binaries, a.datas, name="Questodian")

if sys.platform == "darwin":
    app = BUNDLE(
        coll,
        name="Questodian.app",
        bundle_identifier="app.questodian.desktop",
        info_plist={"NSHighResolutionCapable": True, "CFBundleShortVersionString": "0.1.0"},
    )
