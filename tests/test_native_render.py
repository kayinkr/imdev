import importlib.util
from pathlib import Path
import unittest
from PIL import Image

spec=importlib.util.spec_from_file_location('render_native',Path(__file__).resolve().parents[1]/'tools/render_native.py')
renderer=importlib.util.module_from_spec(spec)
spec.loader.exec_module(renderer)

class NativeRasterTests(unittest.TestCase):
    def test_textured_triangle_uses_vertex_color_and_respects_clip(self):
        mesh={'texture':0,'clip':[0,0,4,8],'vertices':[[0,0,0.5,0.5,[255,0,0,255]],[8,0,0.5,0.5,[255,0,0,255]],[0,8,0.5,0.5,[255,0,0,255]]],'indices':[0,1,2]}
        image=renderer.rasterize(8,8,{0:Image.new('RGBA',(1,1),'white')},[mesh])
        self.assertEqual(image.getpixel((1,1)),(255,0,0,255))
        self.assertEqual(image.getpixel((5,1)),(0,0,0,255))
    def test_font_premultiplied_coverage_is_not_applied_twice(self):
        mesh={'texture':0,'clip':[0,0,8,8],'vertices':[[0,0,0.5,0.5,[255,255,255,255]],[8,0,0.5,0.5,[255,255,255,255]],[0,8,0.5,0.5,[255,255,255,255]]],'indices':[0,1,2]}
        image=renderer.rasterize(8,8,{0:Image.new('RGBA',(1,1),(128,128,128,128))},[mesh])
        self.assertEqual(image.getpixel((1,1)),(128,128,128,255))

if __name__=='__main__':unittest.main()
