# Probe harness

    cargo run --release --bin probe -- [browser.exe] [extra browser args...]
    PROBE_ALL=1 cargo run --release --bin probe     # also list passing checks

Serves `tests/probe/` on a local port, launches the browser (auto-detects Chrome/Edge; or pass an exe such as the
patched Chromium, with a session's switches as extra args), reads `window.__probe` over CDP (no `Runtime.enable`,
because the probe tests for it) and prints non-pass checks plus `tests/probe/last.json`. Exit code 1 on any fail.
Manual: open `http://127.0.0.1:<port>/probe.html` from any static server; the table is on the page.

Groups: `same.*` (values equal in iframe/srcdoc/about:blank/Worker/SharedWorker/ServiceWorker), `lies.*` (native
toString, getter shape, stack leaks, userAgentData), `noise.*` (canvas/audio/rect noise), `x.*` (UA vs hints vs
platform vs WebGL vs fonts vs headers), `auto.*` (webdriver, CDP, headless).

Known harness limits: font and scrollbar checks only know Windows/macOS/Linux font sets (warn, not fail, when
unsure); `auto.outerSize` is a warn and fails legitimately in headless; `x.glVsOS` is a coarse OS-keyword check.

## Public sites to compare manually
- CreepJS: https://abrahamjuliot.github.io/creepjs/ (lies, trust score, worker/iframe mismatch)
- BrowserScan: https://www.browserscan.net/
- Pixelscan: https://pixelscan.net/
- FingerprintJS demo: https://fingerprint.com/demo/
- BrowserLeaks: https://browserleaks.com/ (canvas, webgl, fonts, webrtc, ip)
- bot.sannysoft: https://bot.sannysoft.com/
