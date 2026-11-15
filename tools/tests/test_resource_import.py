import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT=Path(__file__).resolve().parents[1]
def module(name, filename):
    spec=importlib.util.spec_from_file_location(name,ROOT/filename)
    loaded=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded

sketch=module('sketch','import-sketch-resources.py')
normalizer=module('normalizer','normalize-resource-snapshot.py')

class ResourceImportTests(unittest.TestCase):
    def test_original_geometry_and_frontmost_order_survive(self):
        with tempfile.TemporaryDirectory() as folder:
            archive=Path(folder)/'source.sketch'
            root={'_class':'symbolMaster','name':'Original','symbolID':'original',
                  'frame':{'x':0,'y':0,'width':100,'height':50},
                  'style':{'corners':{'radii':[3.4e38]}},'layers':[
                      {'_class':'rectangle','name':name,'frame':{'x':x,'y':2,'width':10,'height':12}}
                      for name,x in [('Background',3),('Foreground',7)]]}
            with zipfile.ZipFile(archive,'w') as z:
                z.writestr('document.json',json.dumps({'pages':[{'_ref':'pages/test'}]}))
                z.writestr('pages/test.json',json.dumps({'layers':[root]}))
            converted=sketch.Importer(archive).node(root)
            self.assertEqual([n['name'] for n in converted['children']],['Foreground','Background'])
            self.assertEqual(converted['children'][0]['x'],7)
            self.assertEqual(converted['cornerRadius'],25)

    def test_colors_reach_the_svg_parser_in_supported_srgb_form(self):
        def color(value):
            return normalizer.re.sub(r'\b(oklab|oklch|lab|lch)\(([^)]+)\)',normalizer.css_rgb,value)
        self.assertEqual(color('lab(100 0 0)'),'rgb(255,255,255)')
        self.assertEqual(color('oklab(0 0 0)'),'rgb(0,0,0)')
        self.assertEqual(color('lab(100 0 0 / 0.5)'),'rgba(255,255,255,0.5)')

if __name__=='__main__': unittest.main()
