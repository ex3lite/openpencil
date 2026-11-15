#!/usr/bin/env python3
"""Import existing Sketch example symbols into editable OpenPencil documents.

No layouts or example content are generated. Original geometry, strings and
styles come from the supplied archive. Keep restricted vendor files local.
"""

import argparse
import base64
import collections
import copy
import hashlib
import json
import re
import zipfile
from pathlib import Path


def pair(value):
    parts = re.findall(r'-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?', str(value))
    return tuple(map(float, parts[:2])) if len(parts) >= 2 else (0, 0)


def color(value):
    channels = [round(max(0, min(1, value.get(k, 0))) * 255) for k in ('red', 'green', 'blue')]
    alpha = round(max(0, min(1, value.get('alpha', 1))) * 255)
    return '#' + ''.join(f'{n:02X}' for n in channels + ([alpha] if alpha < 255 else []))


class Importer:
    def __init__(self, archive):
        self.archive = zipfile.ZipFile(archive)
        self.document = json.loads(self.archive.read('document.json'))
        self.pages = [json.loads(self.archive.read(p['_ref'] + '.json')) for p in self.document['pages']]
        self.symbols = {}
        self.named = {}
        self.shared_styles = {}
        self.warnings = collections.Counter()
        self.next_id = 0
        self.current_page = ''
        self.tokens = {}
        for page in self.pages:
            self.index(page)
        for foreign in self.document.get('foreignSymbols', []):
            self.index(foreign.get('symbolMaster', {}))
        for bucket in ('layerStyles', 'layerTextStyles'):
            for style in self.document.get(bucket, {}).get('objects', []):
                self.shared_styles[style['do_objectID']] = style.get('value', {})
        for swatch in self.document.get('sharedSwatches', {}).get('objects', []):
            self.tokens[swatch['name']] = {'type': 'color', 'value': color(swatch['value'])}

    def index(self, node):
        self.named.setdefault(node.get('name', ''), node)
        if node.get('_class') == 'symbolMaster':
            self.symbols[node['symbolID']] = node
        for child in node.get('layers', []):
            self.index(child)

    def path(self, node, x=0, y=0):
        frame = node.get('frame', {})
        width, height = frame.get('width', 0), frame.get('height', 0)
        points = node.get('points', [])
        if not points:
            return ''

        def xy(point, key='point'):
            px, py = pair(point.get(key, point.get('point')))
            return f'{x + px * width:.5f} {y + py * height:.5f}'

        result = ['M ' + xy(points[0])]
        pairs = list(zip(points, points[1:]))
        if node.get('isClosed'):
            pairs.append((points[-1], points[0]))
        for start, end in pairs:
            if start.get('hasCurveTo') or end.get('hasCurveFrom'):
                result.append('C ' + xy(start, 'curveTo') + ' ' + xy(end, 'curveFrom') + ' ' + xy(end))
            else:
                result.append('L ' + xy(end))
        if node.get('isClosed'):
            result.append('Z')
        return ' '.join(result)

    def fills(self, style):
        result = []
        for fill in style.get('fills', []):
            if not fill.get('isEnabled', True):
                continue
            kind = fill.get('fillType', 0)
            if kind == 0:
                result.append({'type': 'solid', 'color': color(fill.get('color', {}))})
            elif kind == 1:
                gradient = fill.get('gradient', {})
                result.append({'type': 'linear_gradient', 'angle': 180, 'stops': [
                    {'position': s['position'], 'color': color(s['color'])}
                    for s in gradient.get('stops', [])
                ]})
            elif kind == 4:
                ref = fill.get('image', {}).get('_ref', '')
                if ref in self.archive.namelist():
                    data = self.archive.read(ref)
                    mime = 'image/png' if data.startswith(b'\x89PNG') else 'image/jpeg'
                    result.append({'type': 'image', 'url': f'data:{mime};base64,' + base64.b64encode(data).decode()})
                else:
                    self.warnings['missing_pattern_image'] += 1
            else:
                self.warnings['unsupported_fill_' + str(kind)] += 1
        return result

    def apply_overrides(self, root, overrides):
        nodes = {}

        def visit(node, parent=''):
            path = parent + node.get('do_objectID', '')
            nodes[path] = node
            nodes[node.get('do_objectID', '')] = node
            for child in node.get('layers', []):
                visit(child, path + '/')

        visit(root)
        for override in overrides:
            name = override.get('overrideName', '')
            if '_' not in name:
                continue
            path, kind = name.rsplit('_', 1)
            target = nodes.get(path) or nodes.get(path.split('/')[-1])
            if target is None:
                continue
            value = override.get('value')
            if kind == 'stringValue' and isinstance(value, str):
                target.setdefault('attributedString', {})['string'] = value
            elif kind == 'symbolID' and isinstance(value, str):
                target['symbolID'] = value
            elif kind in ('textStyle', 'layerStyle') and value in self.shared_styles:
                target['style'] = copy.deepcopy(self.shared_styles[value])

    def node(self, source, depth=0, inherited=None):
        if depth > 35 or not source.get('isVisible', True):
            return None
        kind = source.get('_class')
        node = source
        if kind == 'symbolInstance':
            master = self.symbols.get(source.get('symbolID'))
            if master is None:
                self.warnings['missing_symbol'] += 1
                return None
            node = copy.deepcopy(master)
            self.apply_overrides(node, source.get('overrideValues', []))
            original = node.get('frame', {})
            instance = source.get('frame', {})
            sx = instance.get('width', 0) / (original.get('width') or 1)
            sy = instance.get('height', 0) / (original.get('height') or 1)
            node['frame'] = instance
            node['name'] = source.get('name', node.get('name'))
            node['rotation'] = source.get('rotation', 0)
            if abs(sx - 1) > .001 or abs(sy - 1) > .001:
                self.resize(node.get('layers', []), sx, sy)
        frame = node.get('frame', {})
        self.next_id += 1
        out = {'type': 'frame', 'id': f'sketch-{self.next_id}', 'name': node.get('name', kind),
               'x': frame.get('x', 0), 'y': frame.get('y', 0),
               'width': frame.get('width', 0), 'height': frame.get('height', 0)}
        if node.get('rotation'):
            out['rotation'] = -node['rotation']
        style = node.get('style') or inherited or {}
        fills = self.fills(style)
        if node.get('hasBackgroundColor') and node.get('backgroundColor'):
            fills.insert(0, {'type': 'solid', 'color': color(node['backgroundColor'])})
        out['fill'] = fills
        alpha = style.get('contextSettings', {}).get('opacity', 1)
        if alpha < 1:
            out['opacity'] = alpha
        borders = [b for b in style.get('borders', []) if b.get('isEnabled')]
        if borders:
            border = borders[0]
            out['stroke'] = {'thickness': border.get('thickness', 1),
                             'fill': [{'type': 'solid', 'color': color(border['color'])}]}
        if kind == 'text':
            attributed = node.get('attributedString', {})
            attrs = style.get('textStyle', {}).get('encodedAttributes', {})
            if not attrs and attributed.get('attributes'):
                attrs = attributed['attributes'][0].get('attributes', {})
            font = attrs.get('MSAttributedStringFontAttribute', {}).get('attributes', {})
            font_name = font.get('name', 'Helvetica')
            weight_name = font_name.lower()
            weight = next((v for k, v in [('ultralight', 100), ('thin', 200), ('light', 300),
                         ('semibold', 600), ('demibold', 600), ('medium', 500), ('heavy', 800),
                         ('black', 900), ('bold', 700)] if k in weight_name), 400)
            font_family = re.sub(r'-(?:Regular|Medium|SemiBold|Semibold|Bold|Light|Thin|Heavy|Black|Italic)$', '', font_name)
            paragraph = attrs.get('paragraphStyle', {})
            out.update(type='text', content=attributed.get('string', ''), fontFamily=font_family,
                       fontSize=font.get('size', 17), fontWeight=weight, textGrowth='fixed-width',
                       fill=[{'type': 'solid', 'color': color(attrs.get('MSAttributedStringColorAttribute',
                                  {'red': 0, 'green': 0, 'blue': 0}))}])
            out['textAlign'] = {1: 'right', 2: 'center', 3: 'justify'}.get(paragraph.get('alignment'), 'left')
            if paragraph.get('maximumLineHeight'):
                out['lineHeight'] = paragraph['maximumLineHeight'] / (font.get('size') or 17)
            if attrs.get('kerning'):
                out['letterSpacing'] = attrs['kerning']
        elif kind in ('rectangle', 'oval', 'shapePath'):
            out['type'] = 'ellipse' if kind == 'oval' else 'rectangle' if kind == 'rectangle' else 'path'
            if kind == 'rectangle':
                radii = [p.get('cornerRadius', 0) for p in node.get('points', [])]
                out['cornerRadius'] = node.get('fixedRadius') or (max(radii) if radii else 0)
            elif kind == 'shapePath':
                out['d'] = self.path(node)
                out['fillRule'] = 'evenodd'
        elif kind == 'bitmap':
            ref = node.get('image', {}).get('_ref', '')
            if ref in self.archive.namelist():
                data = self.archive.read(ref)
                mime = 'image/png' if data.startswith(b'\x89PNG') else 'image/jpeg'
                out.update(type='rectangle', fill=[{'type': 'image',
                           'url': f'data:{mime};base64,' + base64.b64encode(data).decode()}])
            else:
                self.warnings['missing_bitmap'] += 1
        else:
            children = [self.node(c, depth + 1, style if kind == 'shapeGroup' else None)
                        for c in node.get('layers', [])]
            # Sketch paints the last child on top; PenDoc stores topmost first.
            out['children'] = [c for c in reversed(children) if c]
        corners = style.get('corners', {}).get('radii', [])
        if corners and out.get('type') in ('frame', 'rectangle'):
            radius = min(max(corners), min(out['width'], out['height']) / 2)
            out['cornerRadius'] = radius
        return out

    @staticmethod
    def resize(layers, sx, sy):
        for layer in layers:
            f = layer.get('frame', {})
            for key in ('x', 'width'):
                if key in f:
                    f[key] *= sx
            for key in ('y', 'height'):
                if key in f:
                    f[key] *= sy
            Importer.resize(layer.get('layers', []), sx, sy)

    def export(self, names, destination, prefix, source_url):
        destination.mkdir(parents=True, exist_ok=True)
        entries = []
        for name in names:
            master = self.named.get(name)
            if master is None:
                raise ValueError(f'Original symbol not found: {name}')
            self.warnings.clear()
            node = self.node(master)
            node.update(x=0, y=0)
            slug = prefix + '-' + re.sub('[^a-z0-9]+', '-', name.lower()).strip('-')
            doc = {'version': '0.8.5', 'children': [node], 'variables': self.tokens,
                   'editorMeta': {'preserveAuthoredGeometry': True},
                   'resourceSource': {'url': source_url, 'symbol': name, 'importer': 'sketch-source',
                                      'limitations': dict(self.warnings)}}
            path = destination / (slug + '.op')
            path.write_text(json.dumps(doc, ensure_ascii=False, separators=(',', ':')))
            entries.append({'id': slug, 'name': prefix.replace('-', ' ').title() + ' · ' + name,
                            'document': slug + '.op', 'preview': slug + '.jpg',
                            'frames': 1, 'frameWidth': round(node['width']), 'frameHeight': round(node['height']),
                            'source': source_url, 'sourceSymbol': name, 'limitations': dict(self.warnings)})
            print(json.dumps({'file': path.name, 'warnings': dict(self.warnings)}, ensure_ascii=False))
        return entries


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('archive', type=Path)
    p.add_argument('--output', required=True, type=Path)
    p.add_argument('--prefix', required=True)
    p.add_argument('--source-url', required=True)
    p.add_argument('--symbol', action='append', required=True)
    a = p.parse_args()
    entries = Importer(a.archive).export(a.symbol, a.output, a.prefix, a.source_url)
    (a.output / (a.prefix + '-manifest.json')).write_text(json.dumps({
        'sourceSha256': hashlib.sha256(a.archive.read_bytes()).hexdigest(),
        'sourceUrl': a.source_url, 'entries': entries}, ensure_ascii=False, indent=2))
