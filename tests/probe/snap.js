// Collects same-value data in any realm (page, iframe, worker, service worker). Defines self.__snap().
(function () {
  const g = self;
  async function snap() {
    const o = {}, n = g.navigator;
    const t = (k, f) => { try { o[k] = f(); } catch (e) { o[k] = 'ERR ' + e.name; } };
    t('hardwareConcurrency', () => n.hardwareConcurrency);
    t('deviceMemory', () => n.deviceMemory);
    t('platform', () => n.platform);
    t('language', () => n.language);
    t('languages', () => n.languages.join(','));
    t('userAgent', () => n.userAgent);
    t('appVersion', () => n.appVersion);
    t('vendor', () => n.vendor);
    t('maxTouchPoints', () => n.maxTouchPoints);
    t('webdriver', () => n.webdriver);
    if (n.userAgentData) {
      t('uad.brands', () => JSON.stringify(n.userAgentData.brands));
      t('uad.mobile', () => n.userAgentData.mobile);
      t('uad.platform', () => n.userAgentData.platform);
      try {
        const h = await n.userAgentData.getHighEntropyValues(['architecture', 'bitness', 'model', 'platformVersion', 'uaFullVersion', 'fullVersionList', 'wow64']);
        for (const k in h) if (k !== 'brands' && k !== 'mobile' && k !== 'platform') o['uad.' + k] = JSON.stringify(h[k]);
      } catch (e) { o['uad.high'] = 'ERR ' + e.name; }
    }
    if (g.screen) {
      for (const k of ['width', 'height', 'availWidth', 'availHeight', 'colorDepth', 'pixelDepth']) o['screen.' + k] = g.screen[k];
      t('screen.orientation', () => g.screen.orientation && g.screen.orientation.type);
      o.devicePixelRatio = g.devicePixelRatio;
    }
    t('tz.intl', () => Intl.DateTimeFormat().resolvedOptions().timeZone);
    t('tz.offsetNow', () => new Date().getTimezoneOffset());
    t('tz.offsetJan', () => new Date(2024, 0, 15, 12).getTimezoneOffset());
    t('tz.offsetJul', () => new Date(2024, 6, 15, 12).getTimezoneOffset());
    t('locale', () => Intl.DateTimeFormat().resolvedOptions().locale);
    try {
      const c = typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(16, 16) : g.document.createElement('canvas');
      const gl = c.getContext('webgl');
      if (gl) {
        const d = gl.getExtension('WEBGL_debug_renderer_info');
        o['gl.vendor'] = gl.getParameter(gl.VENDOR);
        o['gl.renderer'] = gl.getParameter(gl.RENDERER);
        o['gl.version'] = gl.getParameter(gl.VERSION);
        o['gl.unmaskedVendor'] = d ? gl.getParameter(d.UNMASKED_VENDOR_WEBGL) : null;
        o['gl.unmaskedRenderer'] = d ? gl.getParameter(d.UNMASKED_RENDERER_WEBGL) : null;
        o['gl.extCount'] = (gl.getSupportedExtensions() || []).length;
        for (const k of ['MAX_TEXTURE_SIZE', 'MAX_VIEWPORT_DIMS', 'ALIASED_LINE_WIDTH_RANGE', 'ALIASED_POINT_SIZE_RANGE', 'MAX_VERTEX_ATTRIBS', 'MAX_RENDERBUFFER_SIZE', 'MAX_COMBINED_TEXTURE_IMAGE_UNITS']) {
          const v = gl.getParameter(gl[k]); o['gl.' + k] = v && v.length ? Array.from(v).join(',') : v;
        }
      } else o['gl.vendor'] = null;
    } catch (e) { o['gl.error'] = e.name + ': ' + e.message; }
    return o;
  }
  g.__snap = snap;
  if (typeof WorkerGlobalScope !== 'undefined' && g instanceof WorkerGlobalScope) {
    // Dedicated worker gets onmessage; shared worker gets onconnect.
    const run = (port) => snap().then((s) => port.postMessage(s));
    if (typeof SharedWorkerGlobalScope !== 'undefined' && g instanceof SharedWorkerGlobalScope) g.onconnect = (e) => run(e.ports[0]);
    else g.onmessage = () => run(g);
  }
  if (typeof ServiceWorkerGlobalScope !== 'undefined' && g instanceof ServiceWorkerGlobalScope) {
    g.addEventListener('message', (e) => e.waitUntil(snap().then((s) => e.source.postMessage(s))));
    g.addEventListener('install', () => g.skipWaiting());
    g.addEventListener('activate', (e) => e.waitUntil(g.clients.claim()));
  }
})();
