// Paste into the DevTools console (Chrome/Edge, normal window) on a REAL machine; prints one gl_profiles.json entry.
// Copy the output into chromium/gl_profiles.json under "profiles" with the device id, and set "status": "measured".
(async () => {
  const g = document.createElement('canvas').getContext('webgl2'), g1 = document.createElement('canvas').getContext('webgl');
  const P = n => g.getParameter(g[n]), pair = a => a.join('x');
  const prec = (t, p) => { const f = g.getShaderPrecisionFormat(g[t], g[p]); return `${f.rangeMin}x${f.rangeMax}x${f.precision}`; };
  const r = g.getExtension('WEBGL_debug_renderer_info');
  const m = { tex: 'MAX_TEXTURE_SIZE', cube: 'MAX_CUBE_MAP_TEXTURE_SIZE', rb: 'MAX_RENDERBUFFER_SIZE', va: 'MAX_VERTEX_ATTRIBS', vu: 'MAX_VERTEX_UNIFORM_VECTORS',
    fu: 'MAX_FRAGMENT_UNIFORM_VECTORS', vy: 'MAX_VARYING_VECTORS', ti: 'MAX_TEXTURE_IMAGE_UNITS', vti: 'MAX_VERTEX_TEXTURE_IMAGE_UNITS',
    cti: 'MAX_COMBINED_TEXTURE_IMAGE_UNITS', tex3d: 'MAX_3D_TEXTURE_SIZE', layers: 'MAX_ARRAY_TEXTURE_LAYERS', draw: 'MAX_DRAW_BUFFERS',
    color: 'MAX_COLOR_ATTACHMENTS', samples: 'MAX_SAMPLES', ubb: 'MAX_UNIFORM_BUFFER_BINDINGS' };
  const glmax = [['tex'], ['cube'], ['rb'], ['va'], ['vp', pair(P('MAX_VIEWPORT_DIMS'))], ['lw', pair(P('ALIASED_LINE_WIDTH_RANGE'))], ['ps', pair(P('ALIASED_POINT_SIZE_RANGE'))],
    ['vu'], ['fu'], ['vy'], ['ti'], ['vti'], ['cti'], ['aniso', g.getParameter(g.getExtension('EXT_texture_filter_anisotropic').MAX_TEXTURE_MAX_ANISOTROPY_EXT)],
    ['tex3d'], ['layers'], ['draw'], ['color'], ['samples'], ['ubb']].map(([k, v]) => `${k}:${v ?? P(m[k])}`).join(';');
  const glprec = [['lf', 'LOW_FLOAT'], ['mf', 'MEDIUM_FLOAT'], ['hf', 'HIGH_FLOAT'], ['li', 'LOW_INT'], ['mi', 'MEDIUM_INT'], ['hi', 'HIGH_INT']]
    .map(([k, p]) => `${k}:${prec('FRAGMENT_SHADER', p)}`).join(';');
  const ext = [...new Set([...g1.getSupportedExtensions(), ...g.getSupportedExtensions()])].join(',');
  const a = navigator.gpu && await navigator.gpu.requestAdapter();
  const out = { status: 'measured', source: `${navigator.userAgent} @ ${new Date().toISOString().slice(0, 10)}`, glmax, glprec, glext: ext };
  if (a) Object.assign(out, { wgv: a.info.vendor, wga: a.info.architecture, wgsmin: a.info.subgroupMinSize, wgsmax: a.info.subgroupMaxSize });
  console.log(g.getParameter(r.UNMASKED_RENDERER_WEBGL)); console.log(JSON.stringify(out, null, 2));
})();
