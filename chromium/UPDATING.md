# Updating Chromium (porting the patches)

Patches in `patches/` are applied in name order on a clean checkout of one tag (default in `build.ps1`: `158.0.8127.43`). Upstream moves every ~4 weeks.

1. Pick the new stable tag (e.g. from https://chromiumdash.appspot.com/releases?platform=Windows).
2. Check, without a checkout: `powershell -ExecutionPolicy Bypass -File chromium\port-patches.ps1 -Tag <tag>`
   It fetches only the files the patches touch, applies the patches in order in a scratch repo and prints OK/FAIL per patch (add `-Keep` to keep the repo; failing hunks are left as `.rej`).
3. For each FAIL: open the scratch repo, redo the edit by hand on the new code (the patches are small, anchor lines are quoted in the hunks), then
   `git diff <file>` and replace that file's part of the patch. A file "gone from the tag" was moved/renamed upstream: find it with code search and update the path.
4. Re-run step 2 until every patch is OK. Compile-level breakage (renamed APIs) is only visible in a real build.
5. Bump `$Version` in `build.ps1` and the version note in `README.md`, run `build.ps1`, then rerun the fingerprint checks (README roadmap item 1).

Chromium-vs-Chrome gaps that a source patch cannot close (see `0006-nebula-brand.patch` for the ones that can):
- **Widevine / EME**: `com.widevine.alpha` is unavailable. `enable_widevine` is only default-on for Google-branded builds and needs Google's CDM binary (`library_widevine_cdm_available`, `bundle_widevine_cdm`). Sites that need DRM (Netflix etc.) will fail while a real Chrome plays them. Unsolved; do not copy Chrome's `WidevineCdm` folder in without checking its license.
- **Field trials / variations**: official Chrome gets server-side feature flags; this build runs defaults, so some `chrome://flags`-gated behaviours differ from stable Chrome.
- Already identical to Chrome: `navigator.vendor` ("Google Inc."), the five PDF Viewer plugin names (hardcoded in Blink), `window.chrome`, H.264/AAC (via `proprietary_codecs` + `ffmpeg_branding="Chrome"`).
