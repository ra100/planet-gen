# Cloud map statistics for cloud_capture output.
# usage: python3 analyze.py OUT_DIR SUFFIX [axial_tilt_deg]  (reads clouds-SUFFIX.png, land-SUFFIX.png;
# writes overlay-SUFFIX.png; prints cloud fraction, circulation-latitude profile, histogram, spectrum)
import sys, numpy as np
from PIL import Image
d = sys.argv[1]; suffix = sys.argv[2]
cl = np.asarray(Image.open(f"{d}/clouds-{suffix}.png")).astype(np.float32)/255.0
land = np.asarray(Image.open(f"{d}/land-{suffix}.png")) > 0
H, W = cl.shape
R = cl ** 2.2
# overlay for viewing
base = np.where(land[...,None], np.array([92,80,55],np.float32), np.array([12,26,58],np.float32))
rgb = base*(1-cl[...,None]) + 255*cl[...,None]
Image.fromarray(rgb.clip(0,255).astype(np.uint8)).save(f"{d}/overlay-{suffix}.png")
if H*2 != W:
    print("crop: skipping global stats"); sys.exit()
lat = 90 - (np.arange(H)+0.5)/H*180
wt = np.cos(np.radians(lat))[:,None] * np.ones((1,W))
tilt = np.radians(float(sys.argv[3]) if len(sys.argv) > 3 else 23.44)
lon = (np.arange(W)+0.5)/W*2*np.pi - np.pi
la = np.radians(lat)[:,None]; lo = lon[None,:]
dy = np.sin(la)*np.ones_like(lo); dz = np.cos(la)*np.cos(lo)
clat = np.degrees(np.arcsin(np.clip(dy*np.cos(tilt) + dz*np.sin(tilt), -1, 1)))
cloudy = R > 0.1
def wmean(x, m=None):
    w = wt if m is None else wt*m
    return (x*w).sum()/max(w.sum(),1e-9)
print(f"cloud fraction (R>0.1): global {wmean(cloudy):.3f} ocean {wmean(cloudy,~land):.3f} land {wmean(cloudy,land):.3f}")
print(f"bright fraction (R>0.4): {wmean(R>0.4):.3f}   mean R {wmean(R):.3f}   "
      f"clear (R<0.01): {wmean(R<0.01):.3f}   veil (0.01<R<0.1): {wmean((R>=0.01)&(R<=0.1)):.3f}")
out=[]
for b in range(18):
    hi_, lo_ = 90-b*10, 80-b*10
    m = (clat <= hi_) & (clat > lo_)
    o = m & ~land; l = m & land
    fo = wmean(cloudy, o) if (wt*o).sum() > 0 else float('nan')
    fl = wmean(cloudy, l) if (wt*l).sum() > 0 else float('nan')
    out.append(f"{hi_:+d}:{fo:.2f}/{fl:.2f}")
print("circulation-latitude cloud fraction ocean/land:", " ".join(out))
hist, edges = np.histogram(R, bins=10, range=(0,1), weights=wt)
print("reflectance histogram:", " ".join(f"{h/hist.sum():.2f}" for h in hist))
band = R[H//4:3*H//4]
F = np.abs(np.fft.rfft(band - band.mean(axis=1, keepdims=True), axis=1))**2
spec = F.mean(axis=0); k = np.arange(spec.size)
sel = (k>4)&(k<W//4)
print(f"zonal spectrum slope: {np.polyfit(np.log(k[sel]), np.log(spec[sel]+1e-12), 1)[0]:.2f}  "
      f"variance share < 16px: {spec[k > W/16].sum()/spec[k>0].sum():.4f}")
