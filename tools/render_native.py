"""Software rasterize actual egui meshes + font atlas. Not an OS/GPU screenshot."""
import argparse
import json
import math
from pathlib import Path
from PIL import Image, ImageDraw


def clipped_polygon(points, bounds):
    for axis, edge, lower in [(0,bounds[0],True),(0,bounds[2],False),(1,bounds[1],True),(1,bounds[3],False)]:
        result=[]
        for a,b in zip(points, points[1:]+points[:1]):
            inside_a=a[axis]>=edge if lower else a[axis]<=edge
            inside_b=b[axis]>=edge if lower else b[axis]<=edge
            if inside_a:result.append(a)
            if inside_a!=inside_b:
                ratio=(edge-a[axis])/(b[axis]-a[axis])
                result.append((a[0]+ratio*(b[0]-a[0]),a[1]+ratio*(b[1]-a[1])))
        points=result
        if not points:break
    return points


def rasterize(width,height,textures,meshes):
    canvas=Image.new('RGBA',(width,height),(0,0,0,255))
    draw=ImageDraw.Draw(canvas)
    texture_data={key:(image.width,image.height,image.tobytes()) for key,image in textures.items()}
    for mesh in meshes:
        tw,th,raw=texture_data[mesh['texture']]
        clip=mesh['clip']
        clip=[max(0,clip[0]),max(0,clip[1]),min(width,clip[2]),min(height,clip[3])]
        if clip[2]<=clip[0] or clip[3]<=clip[1]:continue
        def sample(u,v):
            fx=max(0,min(tw-1,u*tw-0.5));fy=max(0,min(th-1,v*th-0.5))
            x=int(fx);y=int(fy);x1=min(tw-1,x+1);y1=min(th-1,y+1);dx=fx-x;dy=fy-y
            a=(y*tw+x)*4;b=(y*tw+x1)*4;c=(y1*tw+x)*4;d=(y1*tw+x1)*4
            return tuple((raw[a+k]*(1-dx)+raw[b+k]*dx)*(1-dy)+(raw[c+k]*(1-dx)+raw[d+k]*dx)*dy for k in range(4))
        vertices=mesh['vertices'];indices=mesh['indices']
        for at in range(0,len(indices),3):
            a,b,c=[vertices[i] for i in indices[at:at+3]]
            area=(b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
            if abs(area)<0.000001:continue
            if area<0:b,c=c,b;area=-area
            same_color=a[4]==b[4]==c[4]
            same_uv=a[2:4]==b[2:4]==c[2:4]
            constant=sample(a[2],a[3]) if same_uv else None
            if same_color and same_uv:
                source=tuple(constant[k]*a[4][k]/255 for k in range(4))
                if source[3]<=0:continue
                if source[3]>=254.99:
                    poly=clipped_polygon([(a[0],a[1]),(b[0],b[1]),(c[0],c[1])],clip)
                    if len(poly)>=3:draw.polygon(poly,fill=tuple(round(x) for x in source))
                    continue
            xmin=max(0,math.ceil(clip[0]-0.5),math.floor(min(a[0],b[0],c[0])))
            xmax=min(width-1,math.ceil(clip[2]-0.5)-1,math.ceil(max(a[0],b[0],c[0])))
            ymin=max(0,math.ceil(clip[1]-0.5),math.floor(min(a[1],b[1],c[1])))
            ymax=min(height-1,math.ceil(clip[3]-0.5)-1,math.ceil(max(a[1],b[1],c[1])))
            pixels=canvas.load()
            edges=[(b,c),(c,a),(a,b)]
            inclusive=[q[1]-p[1]<0 or (q[1]==p[1] and q[0]>p[0]) for p,q in edges]
            for y in range(ymin,ymax+1):
                py=y+0.5
                for x in range(xmin,xmax+1):
                    px=x+0.5
                    weights=[(q[0]-p[0])*(py-p[1])-(q[1]-p[1])*(px-p[0]) for p,q in edges]
                    if any(w < -0.00001 or (abs(w)<0.00001 and not inclusive[i]) for i,w in enumerate(weights)):continue
                    w0,w1,w2=(w/area for w in weights)
                    tex=constant if constant is not None else sample(w0*a[2]+w1*b[2]+w2*c[2],w0*a[3]+w1*b[3]+w2*c[3])
                    color=a[4] if same_color else [w0*a[4][k]+w1*b[4][k]+w2*c[4][k] for k in range(4)]
                    alpha=tex[3]*color[3]/65025
                    if alpha<=0:continue
                    old=pixels[x,y]
                    pixels[x,y]=tuple(max(0,min(255,round(tex[k]*color[k]/255+old[k]*(1-alpha)))) for k in range(3))+(255,)
    return canvas


def render_frame(manifest,output):
    manifest=Path(manifest);scene=json.loads(manifest.read_text());textures={}
    for delta in scene['textures']:
        image=Image.frombytes('RGBA',tuple(delta['size']),(manifest.parent/delta['file']).read_bytes())
        if delta['pos'] is None:textures[delta['id']]=image
        else:textures[delta['id']].paste(image,tuple(delta['pos']))
    image=rasterize(scene['width'],scene['height'],textures,scene['meshes'])
    image.convert('RGB').save(output)
    return image


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest');parser.add_argument('output')
    args=parser.parse_args()
    render_frame(args.manifest,args.output)
    print(args.output)
