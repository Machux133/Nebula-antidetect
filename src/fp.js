(() => {
  const C = __CFG__;
  const def = (o, k, v) => { try { Object.defineProperty(o, k, { get: () => v, configurable: true }); } catch (e) {} };
  const N = Navigator.prototype;

  // Hardware correlation: every value below comes from one device model.
  def(N, 'hardwareConcurrency', C.cores);
  def(N, 'deviceMemory', C.ram);
  def(N, 'platform', C.platform);
  def(N, 'maxTouchPoints', C.touch);
  if (C.lang) { def(N, 'language', C.lang); def(N, 'languages', Object.freeze([C.lang, C.lang.split('-')[0]])); }
  if (C.dnt) def(N, 'doNotTrack', '1');
  const S = Screen.prototype;
  def(S, 'width', C.screen[0]); def(S, 'height', C.screen[1]);
  def(S, 'availWidth', C.screen[0]); def(S, 'availHeight', C.screen[1] - C.taskbar);
  def(S, 'colorDepth', C.depth); def(S, 'pixelDepth', C.depth);

  if (navigator.userAgentData) {
    const U = navigator.userAgentData;
    const he = U.getHighEntropyValues.bind(U);
    const fake = {
      brands: U.brands, mobile: false, platform: C.uaPlatform,
      getHighEntropyValues: async (h) => Object.assign(await he(h), { platform: C.uaPlatform, platformVersion: C.platformVersion, architecture: C.arch, bitness: '64', model: '' }),
      toJSON() { return { brands: U.brands, mobile: false, platform: C.uaPlatform }; }
    };
    def(N, 'userAgentData', fake);
  }

  if (C.tz) {
    const RO = Intl.DateTimeFormat.prototype.resolvedOptions;
    const host = RO.call(new Intl.DateTimeFormat()).timeZone;
    Intl.DateTimeFormat.prototype.resolvedOptions = function () { const r = RO.call(this); if (r.timeZone === host) r.timeZone = C.tz; return r; };
    const fmt = new Intl.DateTimeFormat('en-US', { timeZone: C.tz, hourCycle: 'h23', year: 'numeric', month: 'numeric', day: 'numeric', hour: 'numeric', minute: 'numeric', second: 'numeric' });
    const gt = Date.prototype.getTime;
    Date.prototype.getTimezoneOffset = function () {
      const t = gt.call(this); if (isNaN(t)) return NaN;
      const p = {}; for (const x of fmt.formatToParts(this)) p[x.type] = +x.value;
      return -Math.round((Date.UTC(p.year, p.month - 1, p.day, p.hour, p.minute, p.second) - Math.floor(t / 1000) * 1000) / 60000);
    };
  }

  // WebGL reports the modelled graphics card.
  if (C.webgl) {
    const glp = (P) => { if (!P) return; const g = P.getParameter; P.getParameter = function (p) { if (p === 37445) return C.gpuVendor; if (p === 37446) return C.gpuRenderer; return g.call(this, p); }; };
    glp(window.WebGLRenderingContext && WebGLRenderingContext.prototype);
    glp(window.WebGL2RenderingContext && WebGL2RenderingContext.prototype);
  }

  const mk = (seed) => { let s = (seed >>> 0) || 1; return () => { s ^= s << 13; s ^= s >>> 17; s ^= s << 5; return (s >>> 0) / 4294967296; }; };

  // Canvas: a stable pixel pattern; "Real device" mode uses one seed per device model.
  if (C.canvasSeed !== null) {
    const gI = CanvasRenderingContext2D.prototype.getImageData;
    const shift = (d) => { const r = mk(C.canvasSeed); for (let i = 0; i < d.length; i += 4) { if (r() < 0.02) d[i] ^= 1; } };
    CanvasRenderingContext2D.prototype.getImageData = function (...a) { const d = gI.apply(this, a); shift(d.data); return d; };
    const copy = (src) => {
      const c = document.createElement('canvas'); c.width = src.width; c.height = src.height;
      const x = c.getContext('2d'); x.drawImage(src, 0, 0);
      if (c.width && c.height) { const d = gI.call(x, 0, 0, c.width, c.height); shift(d.data); x.putImageData(d, 0, 0); }
      return c;
    };
    const tD = HTMLCanvasElement.prototype.toDataURL, tB = HTMLCanvasElement.prototype.toBlob;
    HTMLCanvasElement.prototype.toDataURL = function (...a) { try { return tD.apply(copy(this), a); } catch (e) { return tD.apply(this, a); } };
    HTMLCanvasElement.prototype.toBlob = function (...a) { try { return tB.apply(copy(this), a); } catch (e) { return tB.apply(this, a); } };
  }

  // Audio: the real sample rate of the modelled machine, plus a stable per-model curve offset.
  if (C.sampleRate && window.AudioContext) {
    const A = window.AudioContext;
    const W = function (o) { return new A(Object.assign({ sampleRate: C.sampleRate }, o || {})); };
    W.prototype = A.prototype;
    window.AudioContext = W;
  }
  if (C.audioSeed !== null) {
    const off = ((C.audioSeed % 1000) + 1) * 1e-7;
    const g = AudioBuffer.prototype.getChannelData; const seen = new WeakSet();
    AudioBuffer.prototype.getChannelData = function (...a) { const r = g.apply(this, a); if (!seen.has(r)) { seen.add(r); for (let i = 0; i < r.length; i += 97) r[i] += off; } return r; };
    const fq = AnalyserNode.prototype.getFloatFrequencyData;
    AnalyserNode.prototype.getFloatFrequencyData = function (arr) { fq.call(this, arr); for (let i = 0; i < arr.length; i += 7) arr[i] += off * 1000; };
  }

  if (C.rectsSeed !== null) {
    const e = ((C.rectsSeed % 9) + 1) * 1e-6;
    const b = Element.prototype.getBoundingClientRect;
    Element.prototype.getBoundingClientRect = function () { const r = b.call(this); return new DOMRect(r.x + e, r.y + e, r.width + e, r.height + e); };
  }

  if (C.hideWebgpu) def(N, 'gpu', undefined);

  if (C.fakeMedia && window.MediaDevices) {
    const g = 'g' + (C.seed >>> 0).toString(16);
    MediaDevices.prototype.enumerateDevices = async function () {
      return [['audioinput', g], ['videoinput', 'v' + g], ['audiooutput', g]].map(([kind, groupId]) =>
        ({ deviceId: '', kind, label: '', groupId, toJSON() { return { deviceId: '', kind, label: '', groupId }; } }));
    };
  }

  // Fonts: only the modelled system's fonts answer as installed.
  if (C.fonts && C.fonts.length) {
    const allow = new Set(C.fonts.map(f => f.toLowerCase()));
    const generic = new Set(['serif', 'sans-serif', 'monospace', 'cursive', 'fantasy', 'system-ui', 'ui-serif', 'ui-sans-serif', 'ui-monospace', 'emoji', 'math', 'inherit', 'initial']);
    const keep = (x) => { x = x.trim().replace(/^['"]|['"]$/g, '').toLowerCase(); return generic.has(x) || allow.has(x); };
    const split = (f) => String(f).match(/^(.*?\d(?:px|pt|em|rem|%)(?:\/\S+)?\s+)(.+)$/);
    if (window.FontFaceSet) {
      const ck = FontFaceSet.prototype.check;
      FontFaceSet.prototype.check = function (font, txt) { const m = split(font); if (m && !m[2].split(',').every(keep)) return false; return ck.call(this, font, txt); };
    }
    const fd = Object.getOwnPropertyDescriptor(CanvasRenderingContext2D.prototype, 'font');
    Object.defineProperty(CanvasRenderingContext2D.prototype, 'font', {
      configurable: true,
      get() { return fd.get.call(this); },
      set(v) { const m = split(v); if (m) { const k = m[2].split(',').filter(keep); v = m[1] + (k.length ? k.join(',') : 'sans-serif'); } fd.set.call(this, v); }
    });
  }

  // Closed websocket ports: probes of local remote-desktop ports see a closed port.
  if (C.ports && C.ports.length) {
    window.WebSocket = new Proxy(window.WebSocket, {
      construct(t, args) {
        try {
          const u = new URL(args[0], location.href);
          const port = +(u.port || (u.protocol === 'wss:' ? 443 : 80));
          if (/^(localhost|127\.\d+\.\d+\.\d+|\[::1\]|0\.0\.0\.0)$/.test(u.hostname) && C.ports.includes(port)) { u.hostname = '127.0.0.1'; u.port = '1'; args[0] = u.href; }
        } catch (e) {}
        return Reflect.construct(t, args);
      }
    });
  }
})();
