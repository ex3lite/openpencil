#!/usr/bin/env python3
"""Embed public example avatars and normalize modern SVG colors for SVG 1.1."""
import argparse
import base64
import json
import math
import re
import urllib.request
from pathlib import Path


def css_rgb(match):
    kind, body = match.group(1), match.group(2)
    values = [float(x) for x in re.findall(r'[-+]?(?:\d*\.)?\d+(?:e[-+]?\d+)?', body)]
    if len(values) < 3:
        raise ValueError('Invalid CSS color')
    l, a, b = values[:3]
    if kind.endswith('lch'):
        a, b = a * math.cos(math.radians(b)), a * math.sin(math.radians(b))
    if kind.startswith('ok'):
        x, y, z = (l + .3963377774*a + .2158037573*b)**3, (l-.1055613458*a-.0638541728*b)**3, (l-.0894841775*a-1.291485548*b)**3
        rgb = (4.0767416621*x-3.3077115913*y+.2309699292*z,
               -1.2684380046*x+2.6097574011*y-.3413193965*z,
               -.0041960863*x-.7034186147*y+1.707614701*z)
    else:
        fy = (l+16)/116
        def inv(t): return t**3 if t > 6/29 else 3*(6/29)**2*(t-4/29)
        x, y, z = .96422*inv(fy+a/500), inv(fy), .82521*inv(fy-b/200)
        x, y, z = .9555766*x-.0230393*y+.0631636*z, -.0282895*x+1.0099416*y+.0210077*z, .0122982*x-.020483*y+1.3299098*z
        rgb = (3.2404542*x-1.5371385*y-.4985314*z, -.969266*x+1.8760108*y+.041556*z, .0556434*x-.2040259*y+1.0572252*z)
    def channel(v): return round(255*max(0,min(1,12.92*v if v <= .0031308 else 1.055*v**(1/2.4)-.055)))
    parts = ','.join(str(channel(v)) for v in rgb)
    return 'rgba('+parts+','+str(values[3])+')' if len(values)>3 else 'rgb('+parts+')'


def normalize(snapshot):
    def walk(node):
        if not isinstance(node, dict): return
        source = node.get('src', '')
        if source.startswith('data:image/svg+xml,'):
            xml = source.split(',', 1)[1]
        elif source.startswith('data:image/svg+xml;base64,'):
            xml = base64.b64decode(source.split(',', 1)[1]).decode()
        else:
            xml = None
        if xml:
            xml = re.sub(r'\b(oklab|oklch|lab|lch)\(([^)]+)\)', css_rgb, xml)
            xml = re.sub(r'url\(&quot;[^#]*#([^&]+)&quot;\)', r'url(#\1)', xml)
            node['src'] = 'data:image/svg+xml;base64,'+base64.b64encode(xml.encode()).decode()
        elif source.startswith('https://ui.shadcn.com/avatars/'):
            with urllib.request.urlopen(source, timeout=30) as response:
                image = response.read(2*1024*1024+1)
            if len(image)>2*1024*1024: raise ValueError('Avatar exceeds size limit')
            node['src']='data:image/png;base64,'+base64.b64encode(image).decode()
            node.pop('tainted', None)
        for child in node.get('children', []): walk(child)
    walk(snapshot['root'])
    return snapshot


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('snapshot', type=Path)
    args=parser.parse_args()
    data=normalize(json.loads(args.snapshot.read_text()))
    args.snapshot.write_text(json.dumps(data,ensure_ascii=False))
