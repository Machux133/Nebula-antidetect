(() => {
  const C = __CFG__;
  const def = (o, k, v) => { try { Object.defineProperty(o, k, { get: () => v, configurable: true }); } catch (e) {} };
  const N = Navigator.prototype;
  // Timezone, WebGL, canvas, audio noise, text metrics and fonts are native now (chromium/patches 0002-0004, 0007, 0008).


  // Audio: the real sample rate of the modelled machine (JS only: no native patch yet).
  if (C.sampleRate && window.AudioContext) {
    const A = window.AudioContext;
    const W = function (o) { return new A(Object.assign({ sampleRate: C.sampleRate }, o || {})); };
    W.prototype = A.prototype;
    window.AudioContext = W;
  }
  if (C.hideWebgpu) def(N, 'gpu', undefined);

  if (C.fakeMedia && window.MediaDevices) {
    const g = 'g' + (C.seed >>> 0).toString(16);
    MediaDevices.prototype.enumerateDevices = async function () {
      return [['audioinput', g], ['videoinput', 'v' + g], ['audiooutput', g]].map(([kind, groupId]) =>
        ({ deviceId: '', kind, label: '', groupId, toJSON() { return { deviceId: '', kind, label: '', groupId }; } }));
    };
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
