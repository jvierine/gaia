// Display-only exposure normalization. One 32x32 sample per decoded texture.
// The mesh coverage excludes cropped/masked pixels from the histogram.
export const NORMALIZATION_SIZE=32;
export function meshSampleMask(vertices:Float32Array,stride=6):Uint8Array {
  const mask=new Uint8Array(NORMALIZATION_SIZE*NORMALIZATION_SIZE);
  for(let i=0;i+stride<=vertices.length;i+=stride){
    if(stride===6&&!(vertices[i+5]>0))continue;
    const u=vertices[i+3],v=vertices[i+4];if(!Number.isFinite(u)||!Number.isFinite(v))continue;
    const x=Math.max(0,Math.min(31,Math.floor(u*32))),y=Math.max(0,Math.min(31,Math.floor(v*32)));
    mask[y*32+x]=1;
  }
  return mask;
}
export function normalizationGain(rgba:ArrayLike<number>,mask?:Uint8Array):number {
  const histogram=new Uint32Array(256);let count=0;
  for(let i=0;i<rgba.length;i+=4){if(mask&&!mask[i/4]||rgba[i+3]===0)continue;
    const level=Math.max(rgba[i],rgba[i+1],rgba[i+2]);if(level<=1)continue;
    histogram[level]++;count++;
  }
  if(count<8)return 1;
  const target=Math.ceil(count*.95);let sum=0;
  for(let level=2;level<256;level++){sum+=histogram[level];if(sum>=target)return Math.max(1,Math.min(16,180/level));}
  return 1;
}
export function readNormalizationPreference():boolean {try{return localStorage.getItem('gaia-normalize-images')==='true'}catch{return false}}
