// Build a read-only DOM capture expression for official example pages.
// Reuses the engine's snapshot format without clipboard writes or downloads.
import { readFile } from 'node:fs/promises';
export async function resourceSnapshotExpression(rootExpression) {
  const original = await readFile(new URL('../crates/op-html/assets/snapshot-extractor.js', import.meta.url), 'utf8');
  const start = original.indexOf('  function encodeSvg(');
  const end = original.indexOf('  function captureCanvas(', start);
  const svg = `  function encodeSvg(svg, computed, rect) {
    function escape(value) { return String(value).replace(/&/g,'&amp;').replace(/"/g,'&quot;').replace(/</g,'&lt;'); }
    function serialize(node) {
      if (node.nodeType === 3) return escape(node.textContent);
      if (node.nodeType !== 1) return '';
      var attrs = {};
      Array.from(node.attributes).forEach(function(a) { if(a.name !== 'class' && a.name !== 'style') attrs[a.name]=a.value; });
      var style = window.getComputedStyle(node);
      SVG_PAINT_KEYS.concat(['stop-color','stop-opacity']).forEach(function(key) { var value=style.getPropertyValue(key); if(value) attrs[key]=value; });
      if(node===svg) { attrs.xmlns='http://www.w3.org/2000/svg'; attrs.width=rect.width; attrs.height=rect.height; }
      return '<'+node.localName+Object.keys(attrs).map(function(k){return ' '+k+'="'+escape(attrs[k])+'"';}).join('')+'>'+Array.from(node.childNodes).map(serialize).join('')+'</'+node.localName+'>';
    }
    return {src:'data:image/svg+xml,'+serialize(svg)};
  }
\n`;
  let source = original.slice(0,start)+svg+original.slice(end);
  source = source.slice(0,source.indexOf('  if (navigator.clipboard'))+'return JSON.stringify(snapshot);\n})({root:'+rootExpression+'})';
  return '(()=>{const parseFloat=Number.parseFloat,parseInt=Number.parseInt;return '+source+'})()';
}
