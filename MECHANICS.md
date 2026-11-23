# Mechanics sheet

Values read from `Linken Sphere 2.exe` (Ghidra / string table). "estimated" means it was looked for in the code and not found.

## App defaults — function 0x140591ed0 (builds the default settings object)
| Setting | Value | Used in |
|---|---|---|
| customColor (accent) | #1368CA | `ACCENT` in ui.rs |
| warmupAcceptCookies | true | `Settings::warmup_accept_cookies` |
| prolongateCookies | true | `Settings::prolongate_cookies` (stored only) |
| enableIpStats | true | `Settings::enable_ip_stats` |
| ipqsToken | "" | `Settings::ipqs_key` |
| searchEngine | GOOGLE | not used (browser default) |
| closed websocket ports (table 0x14276fd00) | 2112, 3389, 5900-5903, 5279, 5939, 5931, 5944, 5950, 6039, 6040, 7070, 63333, 9602, 6565 | `DEFAULT_CLOSED_PORTS`, applied in fp.js |

## Local API schemas (strings 0x14279b0d0 … 0x14279e250)
- Connection types: notSet, socks5, http, ssh, liveSocks5, liveHttp, direct, tor, localhost, freeProxy; port 0..65535. Rebuilt: direct, http, socks5, tor.
- Start warmup (0x14279ce20): url_count 1..50, time_per_url 0..99 s, view_depth 1..10, urls max 50, accept_cookies defaults to warmupAcceptCookies.
- Create quick sessions (0x14279b0d0): count 1..250.
- Set session name (0x14279d160): 1..50 chars. Description (0x14279db20): up to 65536 chars.
- Fingerprints (0x14279dc80): webgl / canvas / client_rects / audio / webgpu / media_devices each direct|fake; ram and cpu_cores numeric strings.
- Misc (0x14279e250): restore_open_tabs, hardware_acceleration, disable_images, enable_quic, do_not_track (+ others not rebuilt).
- Session status values: running, stopped, imported, warmup, automationRunning.
- Proxy relay: the original points Chromium at `socks5://127.0.0.1:<port>` (string 0x14277c5e0); rebuilt as a local SOCKS5 relay.
- IP fraud lookup: `https://www.ipqualityscore.com/api/json/ip/%s/%s` (0x14277c408) → key, then IP.

## Real-device fingerprints (this rebuild)
Every session points at one device template (devices.rs). Cores, reported memory (Chrome caps at 8), screen sizes, pixel ratio, WebGL vendor/renderer, audio sample rate, OS fonts, UA platform, platform version and CPU architecture all come from that one template, so they never contradict each other. Canvas/audio/client-rect modes:
- Real device: perturbation seeded by the device model id → every session on the same model gives the same hash.
- Noise: seeded per session (unique).
- Off: this computer's value.
Template values come from public model specs (estimated; not captured from live machines).

## Bundled patched Chromium (build.ps1, chromium/patches)
`0001-nebula-device-values.patch` targets Chromium 158.0.8127.43, the version the original ships (chromium-bin/158.0.8127.43). The manager passes `--nebula-fp=cores=..|ram=..|plat=..|pver=..|arch=..|gpuv=..|gpur=..` (browser.rs `nebula_fp`); the browser forwards it to every renderer. Native overrides: navigator.hardwareConcurrency and deviceMemory (pages and workers), WebGL unmasked vendor/renderer, and User-Agent client hints (platform, version, architecture, bitness) in JS and in Sec-CH-UA-* headers. Canvas, audio and fonts still come from the extension.

## Estimated (not found in the code)
- Proxy check timeout 15 s; IP/geo service ip-api.com (the original's service is not in plaintext).
- Warm-up dialog defaults 10 sites / 15 s / depth 2; minimum page load wait 3 s.
- Wait after "change IP" before re-checking: 5 s.
