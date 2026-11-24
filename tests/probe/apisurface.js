(() => {
  const o = {};
  const names = Object.getOwnPropertyNames(window);
  o.window = [...names].sort();
  const ifaces = {};
  for (const n of names) {
    try {
      const v = window[n];
      if (typeof v === 'function' && v.prototype && /^[A-Z]/.test(n)) {
        ifaces[n] = { p: Object.getOwnPropertyNames(v.prototype).sort(), s: Object.getOwnPropertyNames(v).filter((k) => !['length', 'name', 'prototype'].includes(k)).sort() };
      }
    } catch (e) {}
  }
  o.ifaces = ifaces;
  o.css = [...getComputedStyle(document.documentElement)].sort();
  o.navigator = Object.getOwnPropertyNames(navigator).sort();
  o.document = Object.getOwnPropertyNames(document).sort();
  o.secure = isSecureContext;
  return JSON.stringify(o);
})()
