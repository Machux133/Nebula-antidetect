"""Checks chromium/gl_profiles.json against real-browser shares from web3dsurvey.com (chromium/web3dsurvey_webgl.csv).

A profile fails when it lists an extension that under 5 % of real browsers on its OS report, or leaves out one
that over 95 % report. Run: python -I chromium/tests/check_gl_profiles.py
"""
import json
import pathlib

here = pathlib.Path(__file__).resolve().parent.parent
profiles = json.loads((here / "gl_profiles.json").read_text(encoding="utf-8"))["profiles"]
share = {}
for line in (here / "web3dsurvey_webgl.csv").read_text(encoding="utf-8").splitlines():
    if line and not line.startswith("#"):
        name, _page, _all, mac, win, linux = line.split(",")
        share[name] = {"mac": float(mac), "win": float(win), "linux": float(linux)}
os_of = {"mba": "mac", "mbp": "mac", "ima": "mac", "win": "win", "lin": "linux"}
bad = []
for pid, p in profiles.items():
    os = os_of[pid[:3]]
    ext = set(p["glext"].split(","))
    bad += [f"{pid}: lists {e} ({share[e][os]} % on {os})" for e in sorted(ext) if e in share and share[e][os] < 5]
    bad += [f"{pid}: lacks {e} ({s[os]} % on {os})" for e, s in sorted(share.items()) if e not in ext and s[os] > 95]
print("\n".join(bad) or f"ok: {len(profiles)} profiles match web3dsurvey shares")
assert not bad
