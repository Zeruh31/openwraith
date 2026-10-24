// WebHID logger injected into wraith.software before its scripts run.
// Read results with `window.__hidlog`; add separators with `window.__mark(label)`.
(() => {
  const hex = b => Array.from(b instanceof ArrayBuffer ? new Uint8Array(b) : new Uint8Array(b.buffer, b.byteOffset, b.byteLength), x => x.toString(16).padStart(2, '0')).join(' ');
  const log = window.__hidlog = [];
  window.__mark = label => log.push({ t: Date.now(), dir: 'MARK', label });
  const P = window.HIDDevice && HIDDevice.prototype;
  if (!P) return;
  for (const m of ['sendReport', 'sendFeatureReport']) {
    const orig = P[m];
    P[m] = function (id, data) {
      log.push({ t: Date.now(), dir: m === 'sendReport' ? 'OUT' : 'FEAT_OUT', id, data: hex(data) });
      return orig.call(this, id, data);
    };
  }
  const rf = P.receiveFeatureReport;
  P.receiveFeatureReport = async function (id) {
    const r = await rf.call(this, id);
    log.push({ t: Date.now(), dir: 'FEAT_IN', id, data: hex(r) });
    return r;
  };
  const open = P.open;
  P.open = async function () {
    const r = await open.call(this);
    log.push({
      t: Date.now(), dir: 'OPEN', vid: this.vendorId, pid: this.productId, name: this.productName,
      collections: this.collections.map(c => ({ usagePage: c.usagePage, usage: c.usage, out: c.outputReports.map(r => r.reportId), feat: c.featureReports.map(r => r.reportId), inp: c.inputReports.map(r => r.reportId) }))
    });
    this.addEventListener('inputreport', e => log.push({ t: Date.now(), dir: 'IN', id: e.reportId, data: hex(e.data) }));
    return r;
  };
  const close = P.close;
  P.close = function () {
    log.push({ t: Date.now(), dir: 'CLOSE' });
    return close.call(this);
  };
})();
