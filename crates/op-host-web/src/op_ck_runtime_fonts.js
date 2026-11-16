// Exact font bytes from the local supervisor. SkParagraph applies real wght
// variations and intrinsic italic faces; raw Font/Typeface drawText has no
// variation API and changing fvar.defaultValue does not instance outlines.
export function runtimeFontFactory() { return createRuntimeFonts; }

export function selectRuntimeFace(faces, weight, italic) {
  const score = (face) => (face.meta.italic !== Boolean(italic) ? 10000 : 0)
    + Math.max(face.meta.weightMin - weight, 0, weight - face.meta.weightMax);
  return faces.reduce((best, face) => !best || score(face) < score(best) ? face : best, null);
}

function createRuntimeFonts(CK) {
  const families = new Map();
  const seen = new Set();
  const provider = CK.TypefaceFontProvider.Make();
  const paragraphs = new Map();
  const copyBuffer = (bytes) => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  function clearParagraphs() {
    for (const p of paragraphs.values()) p.delete();
    paragraphs.clear();
  }
  function paragraph(entry, text, size, color = [0, 0, 0, 1]) {
    const key = JSON.stringify([entry.key, text, size, color]);
    let p = paragraphs.get(key);
    if (p) return p;
    const style = new CK.ParagraphStyle({
      textStyle: new CK.TextStyle({
        fontFamilies: [entry.face.meta.family], fontSize: size,
        color: CK.Color4f(...color),
        fontStyle: { weight: entry.weight, width: CK.FontWidth.Normal,
          slant: entry.requestedItalic ? CK.FontSlant.Italic : CK.FontSlant.Upright },
        fontVariations: entry.face.meta.variable ? [{ axis: 'wght', value: entry.weight }] : [],
      }),
    });
    const builder = CK.ParagraphBuilder.MakeFromFontProvider(style, provider);
    builder.addText(text);
    p = builder.build();
    builder.delete();
    // A drawText run never wraps; paragraph wrapping is owned by Rust's layout.
    p.layout(1000000);
    if (paragraphs.size >= 256) clearParagraphs();
    paragraphs.set(key, p);
    return p;
  }
  function resolve(familyKey, weight = 400, italic = false) {
    const faces = families.get(familyKey);
    if (!faces) return null;
    const face = selectRuntimeFace(faces, weight, italic);
    const actualWeight = Math.max(face.meta.weightMin, Math.min(face.meta.weightMax, weight));
    const entry = { tf: face.tf, face, key: `runtime:${face.meta.sha256}:${actualWeight}:${italic}`,
      source: 'runtime', actualWeight, weight: actualWeight, italic: face.meta.italic,
      requestedItalic: italic, familyKey };
    entry.measure = (text, size) => paragraph(entry, text, size).getLongestLine();
    entry.metrics = (text, size) => {
      const p = paragraph(entry, text, size);
      return { ascent: p.getAlphabeticBaseline(), descent: p.getHeight() - p.getAlphabeticBaseline() };
    };
    entry.draw = (canvas, text, x, y, size, color) => {
      const p = paragraph(entry, text, size, color);
      canvas.drawParagraph(p, x, y - p.getAlphabeticBaseline());
      return p.getLongestLine();
    };
    return entry;
  }
  function fallback(text, weight = 400, italic = false) {
    // Preserve the existing browser defaults for shell chrome and generic
    // Latin text; the acquired fallback supplies missing Cyrillic glyphs.
    if (!Array.from(text).some((ch) => ch.codePointAt(0) >= 0x400 && ch.codePointAt(0) <= 0x52f)) return null;
    const ordered = [...families.keys()].sort((a, b) => (a === 'noto sans' ? -1 : b === 'noto sans' ? 1 : a.localeCompare(b)));
    for (const family of ordered) {
      const entry = resolve(family, weight, italic);
      if (!entry) continue;
      const ids = entry.tf.getGlyphIDs(text);
      if (ids && ids.length === Array.from(text).length && Array.from(ids).every((id) => id !== 0)) return entry;
    }
    return null;
  }
  function register(bytes, metadata) {
    let meta;
    try { meta = JSON.parse(metadata); } catch (_) { return false; }
    if (seen.has(meta.sha256)) return true;
    const raw = new Uint8Array(bytes).slice();
    const tf = CK.Typeface.MakeFreeTypeFaceFromData(copyBuffer(raw));
    if (!tf || typeof CK.ParagraphBuilder?.MakeFromFontProvider !== 'function') return false;
    // This registration name is the independently parsed intrinsic family,
    // never a caller-supplied alias.
    provider.registerFont(copyBuffer(raw), meta.family);
    const key = String(meta.family).toLowerCase();
    const faces = families.get(key) || [];
    faces.push({ meta, tf });
    families.set(key, faces);
    seen.add(meta.sha256);
    clearParagraphs();
    if (typeof FontFace === 'function' && globalThis.document?.fonts) {
      const font = new FontFace(meta.family, copyBuffer(raw), {
        style: meta.italic ? 'italic' : 'normal',
        weight: meta.variable ? `${meta.weightMin} ${meta.weightMax}` : String(meta.weight),
      });
      font.load().then((loaded) => document.fonts.add(loaded)).catch(() => {});
    }
    return true;
  }
  return { register, resolve, fallback, families: () => [...families.keys()] };
}
