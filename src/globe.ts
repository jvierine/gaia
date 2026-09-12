import {shellTextureCoordinates} from './source-map-coordinates';
import {cameraCompositor} from './camera-compositor';
import {cameraWeightScale} from './composition-rules';
import {liveCutoff} from './live-time';
import {manifestUrl,archiveMode} from './public-manifest';
const VERTEX = `
attribute vec2 position;
void main(){ gl_Position=vec4(position,0.0,1.0); }
`;

const FRAGMENT = `
precision highp float;
uniform vec2 resolution;
uniform vec2 rotation;
uniform float zoom;
uniform vec3 sunDirection;

const float PI=3.141592653589793;
mat3 rotY(float a){float c=cos(a),s=sin(a);return mat3(c,0.,-s,0.,1.,0.,s,0.,c);}
mat3 rotX(float a){float c=cos(a),s=sin(a);return mat3(1.,0.,0.,0.,c,s,0.,-s,c);}
float line(float value,float count,float width){float q=abs(fract(value*count+.5)-.5);return 1.-smoothstep(.001*width,.004*width,q);}
float station(vec2 ll,vec2 target){vec2 d=vec2((ll.x-target.x)*cos(target.y),ll.y-target.y);return 1.-smoothstep(.011,.020,length(d));}
float hash(vec2 p){return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);}

void main(){
  vec2 p=(2.*gl_FragCoord.xy-resolution.xy)/min(resolution.x,resolution.y)/zoom;
  float r2=dot(p,p); vec3 bg=vec3(.003,.012,.022);
  if(r2>1.){gl_FragColor=vec4(bg,1.);return;}
  vec3 n=normalize(vec3(p.x,p.y,sqrt(1.-r2)));
  n=rotY(rotation.x)*rotX(rotation.y)*n;
  float lat=asin(n.y),lon=atan(n.x,n.z); vec2 ll=vec2(lon,lat);
  float limb=pow(max(0.,sqrt(1.-r2)),.42);
  float grid=max(line((lat/PI)+.5,18.,.33),line((lon/(2.*PI))+.5,36.,.26));
  float geoEquator=1.-smoothstep(.0025,.006,abs(lat));
  float oceanNoise=.5+.5*sin(lon*4.+sin(lat*7.))*sin(lat*9.-lon*2.);
  vec3 color=mix(vec3(.010,.055,.085),vec3(.022,.13,.16),oceanNoise*.28);
  color+=grid*vec3(.08,.24,.29);
  color=mix(color,vec3(.16,.60,.72),geoEquator*.62);
  float sites=0.;
  color=mix(color,vec3(.76,1.,.91),sites);
  float sunlight=dot(n,sunDirection);color*=.24+.84*smoothstep(-.04,.10,sunlight);
  float terminator=1.-smoothstep(.003,.012,abs(sunlight));color=mix(color,vec3(1.,.82,.20),terminator*.82);
  float sunMarker=1.-smoothstep(.020,.034,length(n-sunDirection));color=mix(color,vec3(1.,.94,.55),sunMarker);
  color*=limb; float rim=pow(1.-sqrt(1.-r2),7.);color+=rim*vec3(.10,.48,.54);
  gl_FragColor=vec4(color,1.);
}`;

// This is the same explicit GL_LINES overlay used by the HEIMDALL GIM globe.
// GAIA supplies full degree-13 IGRF contours instead of HEIMDALL's old degree-1 circles.
const LINE_VERTEX = `
attribute vec2 lonLat;
uniform vec2 resolution;
uniform vec2 rotation;
uniform float zoom;
varying float visible;
mat3 rotY(float a){float c=cos(a),s=sin(a);return mat3(c,0.,-s,0.,1.,0.,s,0.,c);}
mat3 rotX(float a){float c=cos(a),s=sin(a);return mat3(1.,0.,0.,0.,c,s,0.,-s,c);}
void main(){
  float lon=lonLat.x,lat=lonLat.y;
  vec3 world=vec3(cos(lat)*sin(lon),sin(lat),cos(lat)*cos(lon));
  vec3 camera=rotX(-rotation.y)*rotY(-rotation.x)*world;
  float side=min(resolution.x,resolution.y);
  gl_Position=vec4(camera.x*zoom*side/resolution.x,camera.y*zoom*side/resolution.y,0.,1.);
  visible=camera.z;
}`;
const LINE_FRAGMENT = `
precision highp float;
varying float visible;
uniform vec3 lineColor;
void main(){if(visible<0.)discard;gl_FragColor=vec4(lineColor,1.);}
`;

function shader(gl: WebGLRenderingContext, type: number, source: string) {
  const result = gl.createShader(type)!; gl.shaderSource(result, source); gl.compileShader(result);
  if (!gl.getShaderParameter(result, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(result) || 'Shader failed');
  return result;
}

function makeProgram(gl:WebGLRenderingContext,vertex:string,fragment:string){const p=gl.createProgram()!;gl.attachShader(p,shader(gl,gl.VERTEX_SHADER,vertex));gl.attachShader(p,shader(gl,gl.FRAGMENT_SHADER,fragment));gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw new Error(gl.getProgramInfoLog(p)||'Program link failed');return p}

/** Trace full-IGRF iso-latitudes as line segments, matching HEIMDALL's GL_LINES path. */
function igrfContourVertices(values:Float32Array){
  const width=720,height=361,levels=[-60,-30,0,30,60],vertices:number[]=[];
  const mag=(x:number,y:number)=>values[y*width+(x+width)%width];
  const crossing=(x1:number,y1:number,v1:number,x2:number,y2:number,v2:number,level:number)=>{const d=v2-v1,t=Math.abs(d)<1e-9?.5:(level-v1)/d;return[((x1+(x2-x1)*t)*.5-180)*Math.PI/180,(90-(y1+(y2-y1)*t)*.5)*Math.PI/180]};
  for(let y=0;y<height-1;y++)for(let x=0;x<width;x++)for(const level of levels){
    const x1=x+1,v00=mag(x,y),v10=mag(x1,y),v11=mag(x1,y+1),v01=mag(x,y+1),hits:number[][]=[];
    const edge=(ax:number,ay:number,av:number,bx:number,by:number,bv:number)=>{if((av<level&&bv>=level)||(bv<level&&av>=level))hits.push(crossing(ax,ay,av,bx,by,bv,level))};
    edge(x,y,v00,x1,y,v10);edge(x1,y,v10,x1,y+1,v11);edge(x1,y+1,v11,x,y+1,v01);edge(x,y+1,v01,x,y,v00);
    if(hits.length===2)vertices.push(...hits[0],...hits[1]);else if(hits.length===4)vertices.push(...hits[0],...hits[1],...hits[2],...hits[3]);
  }
  return new Float32Array(vertices);
}

export function startGaiaGlobe(canvas: HTMLCanvasElement,getEpochMillis:()=>number,onLoading:(loading:boolean)=>void=()=>{},publicOnly=false) {
  const gl = canvas.getContext('webgl', { antialias: true }); if (!gl) return () => {};
  const attributeCount=gl.getParameter(gl.MAX_VERTEX_ATTRIBS) as number;
  const resetAttributes=()=>{for(let i=0;i<attributeCount;i++)gl.disableVertexAttribArray(i)};
  const mobilePublic=publicOnly&&matchMedia('(pointer:coarse)').matches;
  const perf=new URLSearchParams(location.search).has('perf')?document.createElement('output'):null;
  if(perf){perf.style.cssText='position:absolute;right:16px;top:70px;color:#9cddc4;font:12px monospace;pointer-events:none';canvas.parentElement?.appendChild(perf)}
  let perfStart=performance.now(),perfFrames=0,perfErrors=0;
  const measure=()=>{if(!perf)return;perfFrames++;const now=performance.now();if(now-perfStart>=1000){if(gl.getError()!==gl.NO_ERROR)perfErrors++;perf.textContent=`${Math.round(perfFrames*1000/(now-perfStart))} display fps · ${perfErrors} WebGL errors`;perfStart=now;perfFrames=0}};
  const program=makeProgram(gl,VERTEX,FRAGMENT),lineProgram=makeProgram(gl,LINE_VERTEX,LINE_FRAGMENT);resetAttributes();gl.useProgram(program);
  const buffer = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, buffer); gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1,3,-1,-1,3]), gl.STATIC_DRAW);
  const position = gl.getAttribLocation(program,'position'); gl.enableVertexAttribArray(position); gl.vertexAttribPointer(position,2,gl.FLOAT,false,0,0);
  const resolution=gl.getUniformLocation(program,'resolution'),rotation=gl.getUniformLocation(program,'rotation'),zoomLoc=gl.getUniformLocation(program,'zoom'),sunLoc=gl.getUniformLocation(program,'sunDirection');
  let yaw=publicOnly?.35:-.35,pitch=publicOnly?-1.15:-.45,zoom=.78,dragging=false,last=[0,0],animation=0;
  // Sun-fixed frame. While locked, yaw and pitch are driven from the sun-earth
  // line every frame: the view looks down on the north pole with the sun upwards and
  // the planet rotating underneath. The manual orientation is restored when unlocked.
  let sunLock=false,manualYaw=yaw,manualPitch=pitch,sunTilt=-Math.PI/2;
  const observationStatus=document.createElement('div');
  observationStatus.style.cssText='position:absolute;left:18px;top:56px;color:#a8c6bf;font:11px monospace;pointer-events:none';
  observationStatus.setAttribute('role','status');canvas.parentElement?.appendChild(observationStatus);
  const archiveLink=document.createElement('a');archiveLink.href=archiveMode?'/gaia/':'/gaia/?archive=1';archiveLink.textContent=archiveMode?'Live view':'Full archive';archiveLink.style.cssText='position:absolute;left:18px;top:76px;color:#a8c6bf;font:11px sans-serif;z-index:5';if(import.meta.env.VITE_GAIA_PUBLIC!=='1'||manifestUrl.startsWith('/gaia/restricted/'))canvas.parentElement?.appendChild(archiveLink);
  type PublicCamera={source_id:string;name:string;producer:string;institution:string;website_url:string;latitude_deg:number|null;longitude_deg:number|null;map_index:number|null;projection?:any;calibrated?:boolean};
  let browserLayers=false,layerManifest:any=null,manifestRefresh:Promise<any>|null=null,manifestFetched=0;
  const compositor=cameraCompositor(gl);
  const published=()=>{if(!manifestRefresh||Date.now()-manifestFetched>30000){manifestFetched=Date.now();manifestRefresh=fetch(manifestUrl,{cache:'no-store',signal:abort.signal}).then(async r=>{if(!r.ok)throw Error('Camera layers unavailable');layerManifest=await r.json();browserLayers=layerManifest.composition==='browser-layers-v1';return layerManifest}).catch(e=>{manifestRefresh=null;throw e})}return manifestRefresh};
  let cameraSites:{source_id?:string;label:string;url?:string;world:number[]}[]=[],publicCameras=new Map<number,PublicCamera>();
  type Attribution={url:string;width:number;height:number;data:Uint8ClampedArray};
  let attribution:Attribution|null=null;
  const attributionCache=new Map<string,Promise<Attribution>>();
  const loadAttribution=(url:string)=>{
    let result=attributionCache.get(url);if(result)return result;
    result=(async()=>{
      let response=await fetch(url,{signal:abort.signal});if(!response.ok)response=await fetch(url,{cache:'reload',signal:abort.signal});if(!response.ok)throw new Error('Attribution map unavailable');
      const objectUrl=URL.createObjectURL(await response.blob()),im=new Image();
      try{await new Promise<void>((resolve,reject)=>{im.onload=()=>resolve();im.onerror=()=>reject(new Error('Attribution decode failed'));im.src=objectUrl;});
        const c=document.createElement('canvas');c.width=im.naturalWidth;c.height=im.naturalHeight;const context=c.getContext('2d',{willReadFrequently:true});if(!context)throw new Error('Attribution canvas unavailable');context.drawImage(im,0,0);
        return {url,width:c.width,height:c.height,data:context.getImageData(0,0,c.width,c.height).data};
      }finally{URL.revokeObjectURL(objectUrl);}
    })().catch(error=>{attributionCache.delete(url);throw error;});
    attributionCache.set(url,result);for(const key of attributionCache.keys()){if(attributionCache.size<=6)break;if(key!==url&&key!==attribution?.url)attributionCache.delete(key);}return result;
  };
  const projectedCamera=(clientX:number,clientY:number)=>{
    if(browserLayers){const r=canvas.getBoundingClientRect();const id=compositor.pick((clientX-r.left)*canvas.width/r.width,(r.bottom-clientY)*canvas.height/r.height,canvas.width,canvas.height,yaw,pitch,zoom,frames,mutedSources);return [...publicCameras.values()].find(c=>c.source_id===id)||null;}
    if(!attribution)return null;const r=canvas.getBoundingClientRect(),side=Math.min(r.width,r.height);
    const px=(2*(clientX-r.left)-r.width)/side/zoom,py=(r.height-2*(clientY-r.top))/side/zoom;
    const uv=shellTextureCoordinates(px,py,yaw,pitch);if(!uv)return null;const [u,v]=uv;
    const x=Math.min(attribution.width-1,Math.floor(u*attribution.width)),y=Math.min(attribution.height-1,Math.max(0,Math.floor(v*attribution.height))),i=(y*attribution.width+x)*4;
    return publicCameras.get((attribution.data[i]<<16)|(attribution.data[i+1]<<8)|attribution.data[i+2])||null;
  };
  const locationLabel=(camera:PublicCamera)=>camera.latitude_deg==null||camera.longitude_deg==null?'location unavailable':`${Math.abs(camera.latitude_deg).toFixed(2)}°${camera.latitude_deg>=0?'N':'S'}, ${Math.abs(camera.longitude_deg).toFixed(2)}°${camera.longitude_deg>=0?'E':'W'}`;
  const stationCamera=(clientX:number,clientY:number,radius=8)=>{
    const r=canvas.getBoundingClientRect(),side=Math.min(r.width,r.height);let best=radius,match:typeof cameraSites[number]|null=null;
    for(const site of cameraSites){const[x,y,z]=site.world,xx=Math.cos(yaw)*x-Math.sin(yaw)*z,zz=Math.sin(yaw)*x+Math.cos(yaw)*z,yy=Math.cos(pitch)*y+Math.sin(pitch)*zz,depth=-Math.sin(pitch)*y+Math.cos(pitch)*zz;if(depth<0)continue;
      const distance=Math.hypot(clientX-r.left-r.width/2-xx*zoom*side/2,clientY-r.top-r.height/2+yy*zoom*side/2);if(distance<best){best=distance;match=site}
    }return match;
  };
  const tooltip=document.createElement('div');tooltip.style.cssText='position:absolute;display:none;pointer-events:none;z-index:5;background:#04111eee;color:white;padding:6px 9px;border:1px solid #54756a;border-radius:4px;font:12px sans-serif';canvas.parentElement?.appendChild(tooltip);
  // Mute is local display state on BOTH sites, never the acquisition switch.
  const mutedSources=new Set<string>();
  try{const saved=JSON.parse(localStorage.getItem('gaia-muted-cameras')||'[]');if(Array.isArray(saved))for(const id of saved)if(typeof id==='string')mutedSources.add(id);}catch{}
  canvas.dataset.mutedCameras=JSON.stringify([...mutedSources]);
  const sourceMapTexture=gl.createTexture()!,visibilityTexture=gl.createTexture()!;
  for(const texture of [sourceMapTexture,visibilityTexture]){gl.bindTexture(gl.TEXTURE_2D,texture);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([255,255,255,255]));}
  let visibilityDirty=true,uploadedSourceMap='';
  let muteTarget:{id:string;name:string}|null=null;
  const toggleMute=(target:{id:string;name:string})=>{
    if(mutedSources.has(target.id))mutedSources.delete(target.id);else mutedSources.add(target.id);
    visibilityDirty=true;
    try{localStorage.setItem('gaia-muted-cameras',JSON.stringify([...mutedSources]));}catch{}
    canvas.dataset.mutedCameras=JSON.stringify([...mutedSources]);
    tooltip.textContent=tooltip.textContent?.replace(/Press "M" to (show|mute) this camera/, 'Press "M" to '+(mutedSources.has(target.id)?'show':'mute')+' this camera')||'';
    observationStatus.textContent=`${target.name}: ${mutedSources.has(target.id)?'muted':'shown'} in this browser. Hold station or press M to toggle.`;
  };
  const muteKey=(e:KeyboardEvent)=>{
    if(e.key.toLowerCase()!=='m'||e.repeat||e.ctrlKey||e.metaKey||e.altKey||!muteTarget)return;
    if(e.target instanceof HTMLElement&&(e.target.isContentEditable||e.target.closest('input:not([type=range]),textarea,select')))return;
    e.preventDefault();toggleMute(muteTarget);
  };
  window.addEventListener('keydown',muteKey);
  const hover=(e:PointerEvent)=>{
    tooltip.style.display='none';muteTarget=null;if(dragging)return;
    const r=canvas.getBoundingClientRect();let label='',url:string|undefined;
    const station=stationCamera(e.clientX,e.clientY),projected=station?null:projectedCamera(e.clientX,e.clientY);
    if(station){label=station.label;url=station.url}else if(projected){label=`Camera: ${projected.name}\nOperator: ${projected.producer}\n${locationLabel(projected)}\nClick for originating provider`;url=projected.website_url}
    const targetId=station?.source_id||projected?.source_id;
    if(targetId){muteTarget={id:targetId,name:station?station.label.split('\n')[0].replace('Camera: ',''):projected!.name};label+='\nPress "M" to '+(mutedSources.has(targetId)?'show':'mute')+' this camera';}
    if(label){tooltip.textContent=label;tooltip.dataset.url=url||'';tooltip.style.whiteSpace='pre-line';tooltip.style.display='block';tooltip.style.left=`${e.clientX-r.left+12}px`;tooltip.style.top=`${e.clientY-r.top+12}px`;}
  };
  const hideTooltip=()=>{tooltip.style.display='none'};
  canvas.addEventListener('pointermove',hover);canvas.addEventListener('pointerleave',hideTooltip);
  const zoomControl=(event:Event)=>{const action=(event as CustomEvent<string>).detail;if(action==='reset'){zoom=.78}else{zoom=Math.max(.5,Math.min(8,zoom*(action==='in'?1.35:1/1.35)))}};
  canvas.addEventListener('gaia-zoom',zoomControl);
  const sunLockControl=(event:Event)=>{const next=!!(event as CustomEvent<boolean>).detail;if(next===sunLock)return;if(next){manualYaw=yaw;manualPitch=pitch}else{yaw=manualYaw;pitch=manualPitch}sunLock=next};
  canvas.addEventListener('gaia-sunlock',sunLockControl);
  const pointers=new Map<number,[number,number]>();let pinchDistance=0,press:[number,number]|null=null,moved=false;
  let holdTimer:ReturnType<typeof setTimeout>|undefined;
  const cancelHold=()=>{if(holdTimer!==undefined)clearTimeout(holdTimer);holdTimer=undefined};
  const suppressNativeMenu=(e:Event)=>e.preventDefault();
  canvas.addEventListener('contextmenu',suppressNativeMenu);
  canvas.addEventListener('selectstart',suppressNativeMenu);
  const distance=()=>{const p=[...pointers.values()];return p.length<2?0:Math.hypot(p[0][0]-p[1][0],p[0][1]-p[1][1])};
  const pointerDown=(e:PointerEvent)=>{
    if(e.pointerType==='mouse'&&e.button!==0)return;e.preventDefault();cancelHold();hideTooltip();pointers.set(e.pointerId,[e.clientX,e.clientY]);dragging=true;last=[e.clientX,e.clientY];press=[e.clientX,e.clientY];moved=pointers.size>1;pinchDistance=distance();canvas.setPointerCapture(e.pointerId);
    const station=e.pointerType==='touch'&&pointers.size===1?stationCamera(e.clientX,e.clientY,14):null;
    if(station?.source_id){const target={id:station.source_id,name:station.label.split('\n')[0].replace('Camera: ','')};holdTimer=setTimeout(()=>{holdTimer=undefined;if(moved||pointers.size!==1||!pointers.has(e.pointerId))return;moved=true;muteTarget=target;toggleMute(target)},550)}
  };
  const pointerMove=(e:PointerEvent)=>{
    if(!pointers.has(e.pointerId))return;e.preventDefault();pointers.set(e.pointerId,[e.clientX,e.clientY]);if(press&&Math.hypot(e.clientX-press[0],e.clientY-press[1])>8){moved=true;cancelHold()}
    if(pointers.size>=2){const d=distance();if(pinchDistance>0&&d>0)zoom=Math.max(.5,Math.min(8,zoom*d/pinchDistance));pinchDistance=d;return}
    if(sunLock){sunTilt=Math.max(-Math.PI+.08,Math.min(-.08,sunTilt-(e.clientY-last[1])*.006));}else{yaw-=(e.clientX-last[0])*.006;pitch=Math.max(-1.35,Math.min(1.35,pitch-(e.clientY-last[1])*.006));}last=[e.clientX,e.clientY];
  };
  const pointerUp=(e:PointerEvent)=>{cancelHold();if(!pointers.has(e.pointerId))return;const tap=e.type==='pointerup'&&!moved&&pointers.size===1&&publicOnly;const station=tap?stationCamera(e.clientX,e.clientY):null,projected=!station&&tap?projectedCamera(e.clientX,e.clientY):null,open=station?.url||projected?.website_url;pointers.delete(e.pointerId);pinchDistance=distance();dragging=pointers.size>0;if(dragging)last=[...pointers.values()][0];else press=null;if(tap&&!dragging)hover(e);if(open)window.open(open,'_blank','noopener,noreferrer')}; const wheel=(e:WheelEvent)=>{e.preventDefault();zoom=Math.max(.5,Math.min(8,zoom*Math.exp(-e.deltaY*.001)))};
  canvas.addEventListener('pointerdown',pointerDown);canvas.addEventListener('pointermove',pointerMove);canvas.addEventListener('pointerup',pointerUp);canvas.addEventListener('wheel',wheel,{passive:false});
  canvas.addEventListener('pointercancel',pointerUp);canvas.addEventListener('lostpointercapture',pointerUp);
  const boundaryBuffer=gl.createBuffer();let boundaryVertices=0;
  void fetch('/gaia/world.geojson').then(r=>r.json()).then(world=>{
    const vertices:number[]=[];
    const ring=(points:number[][])=>{
      for(let i=1;i<points.length;i++){
        const a=points[i-1],b=points[i];let delta=b[0]-a[0];
        if(delta>180)delta-=360;if(delta< -180)delta+=360;
        const steps=Math.max(1,Math.ceil(Math.max(Math.abs(delta),Math.abs(b[1]-a[1]))/.25));
        for(let j=0;j<steps;j++)for(const t of [j/steps,(j+1)/steps])
          vertices.push((a[0]+delta*t)*Math.PI/180,(a[1]+(b[1]-a[1])*t)*Math.PI/180);
      }
    };
    for(const f of world.features){const g=f.geometry;
      if(g.type==='Polygon')for(const r of g.coordinates)ring(r);
      if(g.type==='MultiPolygon')for(const p of g.coordinates)for(const r of p)ring(r);
    }
    boundaryVertices=vertices.length/2;gl.bindBuffer(gl.ARRAY_BUFFER,boundaryBuffer);
    gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(vertices),gl.STATIC_DRAW);
  }).catch(console.warn);
  const magneticBuffer=gl.createBuffer(),magneticPosition=gl.getAttribLocation(lineProgram,'lonLat');let magneticVertices=0;
  const imageProgram=makeProgram(gl,`
attribute vec3 world;attribute vec3 rgb;attribute vec2 uv;varying vec2 texCoord;varying vec3 color;varying float visible;
uniform vec2 resolution;uniform vec2 rotation;uniform float zoom;
mat3 rotY(float a){float c=cos(a),s=sin(a);return mat3(c,0.,-s,0.,1.,0.,s,0.,c);}
mat3 rotX(float a){float c=cos(a),s=sin(a);return mat3(1.,0.,0.,0.,c,s,0.,-s,c);}
void main(){vec3 p=rotX(-rotation.y)*rotY(-rotation.x)*world;float side=min(resolution.x,resolution.y);gl_Position=vec4(p.xy*zoom*side/resolution,0.,1.);visible=p.z;color=rgb;texCoord=uv;}`,`
precision highp float;varying vec2 texCoord;varying vec3 color;varying float visible;uniform sampler2D frame;uniform bool textured;uniform bool applyMute;uniform sampler2D sourceMap;uniform sampler2D cameraVisibility;void main(){if(visible<0.)discard;if(textured){if(applyMute){vec3 code=floor(texture2D(sourceMap,texCoord).rgb*255.+.5);float id=dot(code,vec3(65536.,256.,1.));if(id>0.&&id<65536.&&texture2D(cameraVisibility,vec2(mod(id,256.)+.5,floor(id/256.)+.5)/256.).r<.5)discard;}gl_FragColor=texture2D(frame,texCoord);return;}gl_FragColor=vec4(color,1.);}`);
  type Geometry={buffer:WebGLBuffer;count:number};
  const geometryCache=new Map<string,Geometry>(),textureCache=new Map<string,WebGLTexture>();
  const textureSizes=new Map<WebGLTexture,[number,number]>();
  // Temporal filtering is display-only. Keep the original frames and projected
  // geometry unchanged; blend camera textures at the display refresh rate.
  const smoothProgram=makeProgram(gl,VERTEX,`precision highp float;
uniform sampler2D previous;uniform sampler2D target;uniform vec2 size;uniform float amount;
void main(){vec2 uv=gl_FragCoord.xy/size;gl_FragColor=mix(texture2D(previous,uv),texture2D(target,uv),amount);}`);
  const smoothFramebuffer=gl.createFramebuffer();
  type SmoothState={textures:[WebGLTexture,WebGLTexture];index:number;target:WebGLTexture;geometry:Geometry;changed:number;initialized:boolean;size:[number,number]};
  const smoothStates=new Map<string,SmoothState>();let lastSmoothTime=performance.now(),displayEpoch=getEpochMillis();
  const clearSmooth=()=>{for(const s of smoothStates.values())for(const t of s.textures)gl.deleteTexture(t);smoothStates.clear()};
  let frames:{geometry:Geometry;texture:WebGLTexture;order:number;sourceMapUrl?:string;at:string;sourceId?:string;weightScale?:number}[]=[];
  const layers:{buffer:WebGLBuffer;count:number}[]=[];
  const addLayer=(values:number[]|Float32Array)=>{const b=gl.createBuffer()!;gl.bindBuffer(gl.ARRAY_BUFFER,b);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(values),gl.STATIC_DRAW);layers.push({buffer:b,count:values.length/6})};
  // Expand each station into a small globe-surface disc. WebGL point sprites have
  // implementation-dependent sizing and gl_PointCoord has proved unreliable on
  // some Linux drivers; ordinary triangles use the same reliable path as images.
  const addStationDisc=(values:number[],lat:number,lon:number,color:number[])=>{
    const center=[Math.cos(lat)*Math.sin(lon),Math.sin(lat),Math.cos(lat)*Math.cos(lon)];
    const east=[Math.cos(lon),0,-Math.sin(lon)],north=[-Math.sin(lat)*Math.sin(lon),Math.cos(lat),-Math.sin(lat)*Math.cos(lon)];
    const vertex=(angle:number)=>{const radius=.0045,x=center[0]+radius*(Math.cos(angle)*east[0]+Math.sin(angle)*north[0]),y=center[1]+radius*(Math.cos(angle)*east[1]+Math.sin(angle)*north[1]),z=center[2]+radius*(Math.cos(angle)*east[2]+Math.sin(angle)*north[2]),length=Math.hypot(x,y,z);return[x/length,y/length,z/length]};
    for(let i=0;i<8;i++)values.push(...center,...color,...vertex(i*Math.PI/4),...color,...vertex((i+1)*Math.PI/4),...color);
  };
  const reportBuffer=(detail:{active:boolean;done:number;total:number;failed:number;message?:string;unit?:string})=>canvas.dispatchEvent(new CustomEvent('gaia-buffer-progress',{detail}));
  reportBuffer({active:true,done:0,total:0,failed:0,message:'Loading camera catalogue…'});
  const abort=new AbortController();let frameAbort=new AbortController(),frameTimer=0;
  abort.signal.addEventListener('abort',()=>observationStatus.remove(),{once:true});
  abort.signal.addEventListener('abort',()=>archiveLink.remove(),{once:true});
  void (publicOnly?published().then(async manifest=>{const cameras=manifest.cameras||[];publicCameras=new Map(cameras.filter(c=>c.map_index!=null).map(c=>[c.map_index!,c]));cameraSites=cameras.filter(c=>c.latitude_deg!==null&&c.longitude_deg!==null).map(c=>{const lat=c.latitude_deg!*Math.PI/180,lon=c.longitude_deg!*Math.PI/180;return{source_id:c.source_id,label:`Camera: ${c.name}\nOperator: ${c.producer}\n${locationLabel(c)}\nClick for originating provider`,url:c.website_url,world:[Math.cos(lat)*Math.sin(lon),Math.sin(lat),Math.cos(lat)*Math.cos(lon)]}});const sites:number[]=[];for(const camera of cameras){if(camera.latitude_deg===null||camera.longitude_deg===null)continue;addStationDisc(sites,camera.latitude_deg*Math.PI/180,camera.longitude_deg*Math.PI/180,[.56,.76,.69]);}addLayer(sites);return browserLayers?cameras.map((c:PublicCamera)=>({id:c.source_id,name:c.name,producer:c.producer,calibrated:!!c.projection,enabled:true,latitude_deg:null,longitude_deg:null})):[{id:'composite',name:'Composite',producer:'See credits',calibrated:true,enabled:true,latitude_deg:null,longitude_deg:null}]}):fetch('/gaia/api/sources',{cache:'no-store',signal:abort.signal}).then(r=>r.json())).then(async (sources:{id:string;name:string;producer:string;calibrated:boolean;enabled:boolean;latitude_deg:number|null;longitude_deg:number|null}[])=>{
    const focus=sources.find(s=>s.enabled&&s.calibrated&&s.latitude_deg!==null&&s.longitude_deg!==null);if(focus){yaw=focus.longitude_deg!*Math.PI/180;pitch=-focus.latitude_deg!*Math.PI/180;}
    const sites:number[]=[];for(const s of sources){if(!s.enabled||s.latitude_deg===null||s.longitude_deg===null)continue;addStationDisc(sites,s.latitude_deg*Math.PI/180,s.longitude_deg*Math.PI/180,s.calibrated?[.3,.82,.58]:[.92,.3,.34]);}
    addLayer(sites);
    if(!publicOnly)cameraSites=sources.filter(s=>s.enabled&&s.latitude_deg!==null&&s.longitude_deg!==null).map(s=>{const lat=s.latitude_deg!*Math.PI/180,lon=s.longitude_deg!*Math.PI/180;return{source_id:s.id,label:`Camera: ${s.name}\nOperator: ${s.producer}`,world:[Math.cos(lat)*Math.sin(lon),Math.sin(lat),Math.cos(lat)*Math.cos(lon)]}});
    let lastMinute=-1,lastRefresh=0,selection=0,activeLoads=0,lastPrune=0;
    const cameraOrders=new Map<string,number>();
    const cameraOrder=(id:string)=>{if(!cameraOrders.has(id))cameraOrders.set(id,cameraOrders.size);return cameraOrders.get(id)!};
    type Asset={geometry_url:string;texture_url:string;vertex_count:number;weight_scale?:number};
    type Catalogue=Asset&{width:number;height:number;images:{source_id?:string;at:string;width:number;height:number;texture_url:string;source_map_url?:string;geometry_url?:string;vertex_count?:number;weight_scale?:number}[]};
    const catalogues=new Map<string,{expires:number;value:Promise<Catalogue|null>}>();
    const catalogue=(id:string)=>{
      let c=catalogues.get(id);if(!c||c.expires<Date.now()){
        if(publicOnly){c={expires:Date.now()+30000,value:published().then(m=>{publicCameras=new Map((m.cameras||[]).filter((c:PublicCamera)=>c.map_index!=null).map((c:PublicCamera)=>[c.map_index!,c]));return browserLayers?m.cameras.find((c:PublicCamera)=>c.source_id===id)?.projection||null:m})};catalogues.set(id,c);return c.value;}
        c={expires:Date.now()+60000,value:fetch(publicOnly?manifestUrl:`/gaia/api/sources/${encodeURIComponent(id)}/projection?format=timeline`,{cache:'no-store',signal:abort.signal}).then(async r=>{if(r.status===404)return null;if(!r.ok)throw new Error('Playback catalogue unavailable');const cat=await r.json() as Catalogue & {cameras?:PublicCamera[]};if(cat.cameras){publicCameras=new Map(cat.cameras.filter(c=>c.map_index!=null).map(c=>[c.map_index!,c]));visibilityDirty=true;}return cat})};catalogues.set(id,c);
      }return c.value;
    };
    let overview:any=null,warming=false,overviewActive=false,overviewComplete=false,playAnimation=0,warmRequest:AbortController|null=null;
    const sheetImages=new Map<string,Promise<HTMLImageElement>>();
    const progress=new Map<number,{done:number;total:number;failed:number}>();
    const emitProgress=(minute:number)=>{if(!warming&&!overviewActive&&minute===Math.floor(getEpochMillis()/60000))reportBuffer({active:true,...(progress.get(minute)||{done:0,total:0,failed:0})})};
    const pending=new Map<number,Promise<typeof frames>>(),ready=new Map<number,typeof frames>(),requests=new Map<number,AbortController>();
    const geometryDownloads=new Map<string,Promise<ArrayBuffer>>(),textureDownloads=new Map<string,Promise<Blob>>();
    const sharedDownload=<T,>(cache:Map<string,Promise<T>>,url:string,load:()=>Promise<T>)=>{let p=cache.get(url);if(!p){p=load().finally(()=>cache.delete(url));cache.set(url,p)}return p};
    abort.signal.addEventListener('abort',()=>{for(const c of requests.values())c.abort()},{once:true});
    const loadFrames=async(epoch:number,request:AbortController)=>{
      activeLoads++;
      try {
      const nextFrames:typeof frames=[];
      const snapshot=overviewActive?overview:browserLayers?await published():null;
      const selectedSources=snapshot?snapshot.cameras.filter((c:PublicCamera)=>c.projection).map((c:PublicCamera)=>({id:c.source_id,name:c.name,producer:c.producer,calibrated:true,enabled:true})):sources;
      const queue=selectedSources.filter(s=>s.enabled&&s.calibrated).map(s=>({...s,order:cameraOrder(s.id)}));
      const progressMinute=Math.floor(epoch/60000),state={done:0,total:queue.length,failed:0};
      progress.set(progressMinute,state);emitProgress(progressMinute);
      const at=new Date(epoch).toISOString();
      await Promise.all([0,1,2,3,4,5].map(async()=>{while(queue.length&&!request.signal.aborted){
        const s=queue.shift()!;
        try{
          const cat:Catalogue|null=snapshot?snapshot.cameras.find((c:PublicCamera)=>c.source_id===s.id)?.projection:await catalogue(s.id);if(!cat)continue;
          let frame:Catalogue['images'][number]|undefined;
          for(let i=cat.images.length-1;i>=0;i--){if(Date.parse(cat.images[i].at)<=epoch){frame=cat.images[i];break}}
          // Retain the latest published frame during live delivery delays.
          // Historical playback still preserves real observation gaps.
          const live=publicOnly&&Math.abs(liveCutoff()-epoch)<120000;
          if(!frame||((browserLayers||!live)&&epoch-Date.parse(frame.at)>600000))continue;
          if(frame.source_id&&frame.source_id!==s.id)throw Error('Rejected frame belonging to another station');
          let asset:Asset={...cat,...frame,texture_url:frame.texture_url} as Asset;
          if(!browserLayers&&(frame.width!==cat.width||frame.height!==cat.height)){
            const r=await fetch(`/gaia/api/sources/${encodeURIComponent(s.id)}/projection?format=assets&at=${encodeURIComponent(at)}`,{cache:'no-store',signal:request.signal});
            if(r.status===404)continue;if(!r.ok)throw new Error(await r.text());asset=await r.json();
          }
          const rect=(frame as any).texture_rect as number[]|undefined;
          const textureKey=asset.texture_url+(rect?'#'+rect.join(','):'');
          let geometry=geometryCache.get(asset.geometry_url),texture=textureCache.get(textureKey);
          const [bytes,blob]=await Promise.all([
            geometry?null:sharedDownload(geometryDownloads,asset.geometry_url,()=>fetch(asset.geometry_url,{signal:abort.signal}).then(r=>{if(!r.ok)throw new Error('Geometry unavailable');return r.arrayBuffer()})),
            texture||rect?null:sharedDownload(textureDownloads,asset.texture_url,()=>fetch(asset.texture_url,{signal:abort.signal}).then(async r=>{if(!r.ok)r=await fetch(asset.texture_url,{cache:'reload',signal:abort.signal});if(!r.ok)throw new Error('Texture unavailable');return r.blob()}))
          ]);
          if(request.signal.aborted||abort.signal.aborted)return;
          geometry=geometryCache.get(asset.geometry_url)||geometry;
          if(!geometry&&bytes){const b=gl.createBuffer()!;gl.bindBuffer(gl.ARRAY_BUFFER,b);gl.bufferData(gl.ARRAY_BUFFER,bytes,gl.STATIC_DRAW);geometry={buffer:b,count:bytes.byteLength/(browserLayers?24:20)};geometryCache.set(asset.geometry_url,geometry)}
          if(!texture&&rect){
            let decoded=sheetImages.get(asset.texture_url);
            if(!decoded){decoded=fetch(asset.texture_url,{signal:request.signal}).then(async r=>{if(!r.ok)throw Error('Overview sheet unavailable');const url=URL.createObjectURL(await r.blob()),im=new Image();try{await new Promise<void>((resolve,reject)=>{im.onload=()=>resolve();im.onerror=()=>reject(Error('Overview decode failed'));im.src=url});return im}finally{URL.revokeObjectURL(url)}});sheetImages.set(asset.texture_url,decoded);decoded.catch(()=>sheetImages.delete(asset.texture_url))}
            const im=await decoded;if(request.signal.aborted)return;
            texture=textureCache.get(textureKey);
            if(!texture){const tile=document.createElement('canvas');tile.width=64;tile.height=64;tile.getContext('2d')!.drawImage(im,rect[0],rect[1],rect[2],rect[3],0,0,64,64);
              texture=gl.createTexture()!;gl.bindTexture(gl.TEXTURE_2D,texture);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,gl.RGBA,gl.UNSIGNED_BYTE,tile);textureCache.set(textureKey,texture);textureSizes.set(texture,[64,64]);}
          }
          if(!texture&&blob){
            const url=URL.createObjectURL(blob),im=new Image();
            try{await new Promise<void>((resolve,reject)=>{im.onload=()=>resolve();im.onerror=()=>reject(new Error('Texture decode failed'));im.src=url});
              if(request.signal.aborted||abort.signal.aborted)return;
              texture=textureCache.get(asset.texture_url);
              if(!texture){texture=gl.createTexture()!;gl.bindTexture(gl.TEXTURE_2D,texture);
              gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);
              gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);
              const limit=Math.min(gl.getParameter(gl.MAX_TEXTURE_SIZE) as number,mobilePublic?2048:8192);
              const scale=Math.min(1,limit/Math.max(im.naturalWidth,im.naturalHeight));
              let upload:HTMLImageElement|HTMLCanvasElement=im;
              if(scale<1){const reduced=document.createElement('canvas');reduced.width=Math.max(1,Math.round(im.naturalWidth*scale));reduced.height=Math.max(1,Math.round(im.naturalHeight*scale));reduced.getContext('2d')!.drawImage(im,0,0,reduced.width,reduced.height);upload=reduced;}
              gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,gl.RGBA,gl.UNSIGNED_BYTE,upload);textureCache.set(asset.texture_url,texture);textureSizes.set(texture,[upload.width,upload.height]);}
            }finally{URL.revokeObjectURL(url)}
          }
          // Never display an image with the preceding frame's camera map.
          if(frame.source_map_url)await loadAttribution(frame.source_map_url);
          const camera=snapshot?.cameras?.find((c:PublicCamera)=>c.source_id===s.id);
          const weightScale=browserLayers&&camera&&snapshot?.stitching?.rules?cameraWeightScale(camera,epoch,snapshot.stitching.rules):asset.weight_scale;
          if(geometry&&texture&&gl.isBuffer(geometry.buffer)&&gl.isTexture(texture))nextFrames.push({geometry,texture,order:s.order,sourceMapUrl:frame.source_map_url,at:(frame as any).observation_at||frame.at,sourceId:s.id,weightScale});
        }catch(e){if(!request.signal.aborted){state.failed++;console.error(`Projection ${s.name}`,e)}}
        finally{state.done++;if(!request.signal.aborted)emitProgress(progressMinute)}
      }}));
      return nextFrames.sort((a,b)=>a.order-b.order);
      } finally {activeLoads--}
    };
    const prepare=(epoch:number)=>{
      const minute=Math.floor(epoch/60000);let p=pending.get(minute);
      if(!p){const request=new AbortController();requests.set(minute,request);p=loadFrames(minute*60000,request).then(result=>{if(!request.signal.aborted&&requests.get(minute)===request)ready.set(minute,result);return result}).catch(e=>{if(!request.signal.aborted)console.warn('Frame preparation failed',e);if(requests.get(minute)===request){pending.delete(minute);ready.delete(minute)}return []});pending.set(minute,p)}return p;
    };
    const updateFrames=async()=>{
      if(warming)return;
      // In-flight loads hold local GPU references across asynchronous decode.
      // Never delete those objects: binding a deleted texture can leave the
      // preceding camera's binding active and display that camera's image.
      if(activeLoads===0&&Date.now()-lastPrune>5000){lastPrune=Date.now();
        const protectedFrames=[...frames,...[...ready.values()].flat()],protectedTextures=new Set(protectedFrames.map(f=>f.texture)),protectedGeometry=new Set(protectedFrames.map(f=>f.geometry));
        for(const [key,t] of textureCache){if(textureCache.size<=(browserLayers?(mobilePublic?384:768):96))break;if(!protectedTextures.has(t)){gl.deleteTexture(t);textureSizes.delete(t);textureCache.delete(key)}}
        for(const [key,g] of geometryCache){if(geometryCache.size<=256)break;if(!protectedGeometry.has(g)){gl.deleteBuffer(g.buffer);geometryCache.delete(key)}}
      }
      const requestedEpoch=getEpochMillis(),epoch=overviewActive&&overviewComplete?overview.images.map((f:any)=>Date.parse(f.at)).reduce((a:number,b:number)=>Math.abs(a-requestedEpoch)<Math.abs(b-requestedEpoch)?a:b):requestedEpoch,minute=Math.floor(epoch/60000),refresh=!overviewActive&&Date.now()-lastRefresh>=30000;
      if(minute===lastMinute&&!refresh)return;
      if(refresh){lastRefresh=Date.now();catalogues.clear();if(minute===lastMinute){requests.get(minute)?.abort();requests.delete(minute);pending.delete(minute);ready.delete(minute)}}
      lastMinute=minute;
      const ticket=++selection;
      for(const key of pending.keys())if(!overviewActive&&key!==minute){requests.get(key)?.abort();requests.delete(key);pending.delete(key);ready.delete(key);progress.delete(key)}
      onLoading(!ready.has(minute));
      if(!ready.has(minute))emitProgress(minute);
      const next=await prepare(epoch);
      if(selection===ticket&&Math.floor(getEpochMillis()/60000)===Math.floor(requestedEpoch/60000)&&!abort.signal.aborted){
        if(epoch<displayEpoch||(!overviewActive&&Math.abs(epoch-displayEpoch)>600000)){clearSmooth();displayEpoch=epoch}
        if(overviewActive||next.length||Math.abs(liveCutoff()-epoch)>=120000)frames=next;
        canvas.dataset.selectedEpoch=String(minute*60000);
        canvas.dataset.cameraOrderUnique=String(new Set(frames.map(f=>f.order)).size===frames.length);
        const shownAt=frames[0]?.at;
        if(shownAt){canvas.dataset.observationUtc=shownAt;canvas.dataset.delayed=String(epoch-Date.parse(shownAt)>600000);observationStatus.textContent=`Image ${new Date(shownAt).toISOString().slice(11,16)} UTC${epoch-Date.parse(shownAt)>600000?' · delayed':''}`;}
        const sourceMapUrl=frames[0]?.sourceMapUrl;if(sourceMapUrl)void loadAttribution(sourceMapUrl).then(value=>{if(frames[0]?.sourceMapUrl===value.url)attribution=value}).catch(console.warn);else attribution=null;
        // Decode ahead without destroying the buffer on every catalogue refresh.
        // Whole-day overview is already GPU resident; no per-frame downloads or speculative minute loads.
        onLoading(false);
        const status=progress.get(minute)||{done:0,total:0,failed:0};
        if(!warming)reportBuffer({active:false,...status});
      }
    };
    const cancelBuffer=()=>{cancelAnimationFrame(playAnimation);warmRequest?.abort();warming=false;overviewActive=false;overviewComplete=false;sheetImages.clear();selection++;for(const request of requests.values())request.abort();requests.clear();pending.clear();ready.clear();progress.clear();onLoading(false);reportBuffer({active:false,done:0,total:0,failed:0})};
    canvas.addEventListener('gaia-cancel-buffer',cancelBuffer);
    abort.signal.addEventListener('abort',()=>canvas.removeEventListener('gaia-cancel-buffer',cancelBuffer),{once:true});
    const playOverview=async(event:Event)=>{
      const {active,speed=32}=(event as CustomEvent).detail;
      cancelAnimationFrame(playAnimation);
      if(!active){if(warming){warmRequest?.abort();warming=false;overviewComplete=false;overviewActive=false;lastMinute=-1;reportBuffer({active:false,done:0,total:0,failed:0})}return}
      try{
        if(!overviewComplete){
          warmRequest?.abort();const request=new AbortController();warmRequest=request;warming=true;
          selection++;for(const r of requests.values())r.abort();requests.clear();pending.clear();ready.clear();progress.clear();
          reportBuffer({active:true,done:0,total:120,failed:0,unit:'overview frames',message:'Buffering the entire 24-hour overview…'});
          const manifest=await published();if(!manifest.overview)throw Error('Full-day overview is not published yet. Please retry shortly.');
          overview=manifest.overview;overviewActive=true;window.dispatchEvent(new CustomEvent('gaia-overview-catalogue',{detail:overview.images}));
          const times=overview.images.map((f:any)=>Date.parse(f.at));
          canvas.dataset.overviewReady='false';
          for(let i=0;i<times.length;i++){
            if(request.signal.aborted||abort.signal.aborted)return;
            const epoch=times[i],minute=Math.floor(epoch/60000),result=await loadFrames(epoch,request);
            if(request.signal.aborted||abort.signal.aborted)return;
            if(progress.get(minute)?.failed)throw Error('Some overview images failed to load. Retry buffering; playback has not started.');
            ready.set(minute,result);pending.set(minute,Promise.resolve(result));
            reportBuffer({active:true,done:i+1,total:times.length,failed:0,unit:'overview frames',message:'Buffering the entire 24-hour overview…'});
            await new Promise(resolve=>setTimeout(resolve,0));
          }
          sheetImages.clear();warming=false;overviewComplete=true;lastMinute=-1;
          canvas.dataset.overviewReady='true';canvas.dataset.overviewFrames=String(times.length);canvas.dataset.overviewTexturePixels='64';
          reportBuffer({active:false,done:times.length,total:times.length,failed:0});
        }
        const times=overview.images.map((f:any)=>Date.parse(f.at)),duration=15000*32/Math.max(.25,speed),start=performance.now();let previous=-1,loops=0;
        canvas.dataset.playbackStarted=String(start);canvas.dataset.playbackDuration=String(duration);
        const tick=(now:number)=>{if(abort.signal.aborted)return;const elapsed=now-start,loop=Math.floor(elapsed/duration),i=Math.min(times.length-1,Math.floor(elapsed%duration/duration*times.length));
          if(loop>loops){canvas.dataset.playbackLoopMs=String(elapsed/loop);loops=loop}
          if(i!==previous){previous=i;window.dispatchEvent(new CustomEvent('gaia-overview-epoch',{detail:times[i]}))}
          playAnimation=requestAnimationFrame(tick);
        };playAnimation=requestAnimationFrame(tick);
      }catch(error){warming=false;overviewActive=false;overviewComplete=false;lastMinute=-1;reportBuffer({active:false,done:0,total:0,failed:1,message:String((error as Error).message)});window.dispatchEvent(new Event('gaia-pause-playback'))}
    };
    const leaveOverview=()=>{cancelAnimationFrame(playAnimation);overviewActive=false;overviewComplete=false;warmRequest?.abort();warming=false;pending.clear();ready.clear();lastMinute=-1};
    const scrubOverview=(event:Event)=>{if(!overviewComplete)return;const wanted=(event as CustomEvent).detail;const t=overview.images.map((f:any)=>Date.parse(f.at)).reduce((a:number,b:number)=>Math.abs(a-wanted)<Math.abs(b-wanted)?a:b);window.dispatchEvent(new CustomEvent('gaia-overview-epoch',{detail:t}))};
    window.addEventListener('gaia-scrub-overview',scrubOverview);
    abort.signal.addEventListener('abort',()=>window.removeEventListener('gaia-scrub-overview',scrubOverview),{once:true});
    window.addEventListener('gaia-run-overview',playOverview);window.addEventListener('gaia-leave-overview',leaveOverview);
    abort.signal.addEventListener('abort',()=>{cancelAnimationFrame(playAnimation);warmRequest?.abort();window.removeEventListener('gaia-run-overview',playOverview);window.removeEventListener('gaia-leave-overview',leaveOverview)},{once:true});
    void updateFrames();frameTimer=window.setInterval(()=>void updateFrames(),16);
  }).catch(error=>{console.error(error);onLoading(false);reportBuffer({active:false,done:0,total:0,failed:1,message:'Camera data could not be loaded. Please retry.'})});
  // Final overlay pass: measured image colors are unlit and opaque, above both
  // the Earth's night shading and IGRF lines. The shader still hides the far side.
  const drawLayers=(w:number,h:number)=>{
    const depthTest=gl.isEnabled(gl.DEPTH_TEST),blend=gl.isEnabled(gl.BLEND);
    gl.disable(gl.DEPTH_TEST);gl.disable(gl.BLEND);
    const now=performance.now(),dt=Math.min(100,now-lastSmoothTime);lastSmoothTime=now;
    const amount=1-Math.exp(-dt/65),displayTextures=new Map<string,WebGLTexture>();
    const stationKey=(frame:typeof frames[number])=>frame.sourceId??String(frame.order);
    for(const [key,s] of smoothStates)if(!frames.some(f=>stationKey(f)===key)){for(const t of s.textures)gl.deleteTexture(t);smoothStates.delete(key)}
    resetAttributes();gl.useProgram(smoothProgram);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
    const a=gl.getAttribLocation(smoothProgram,'position');gl.enableVertexAttribArray(a);gl.vertexAttribPointer(a,2,gl.FLOAT,false,0,0);
    gl.uniform1i(gl.getUniformLocation(smoothProgram,'previous'),0);gl.uniform1i(gl.getUniformLocation(smoothProgram,'target'),1);
    for(const frame of frames){
      const size=textureSizes.get(frame.texture);if(!size){displayTextures.set(stationKey(frame),frame.texture);continue}
      let state=smoothStates.get(stationKey(frame));
      if(state&&(state.geometry!==frame.geometry||state.size[0]!==size[0]||state.size[1]!==size[1])){for(const t of state.textures)gl.deleteTexture(t);smoothStates.delete(stationKey(frame));state=undefined}
      if(!state){
        const textures=[0,1].map(()=>{const t=gl.createTexture()!;gl.bindTexture(gl.TEXTURE_2D,t);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,size[0],size[1],0,gl.RGBA,gl.UNSIGNED_BYTE,null);return t}) as [WebGLTexture,WebGLTexture];
        state={textures,index:0,target:frame.texture,geometry:frame.geometry,changed:now,initialized:false,size};smoothStates.set(stationKey(frame),state);
      }
      if(state.target!==frame.texture){state.target=frame.texture;state.changed=now}
      if(state.initialized&&now-state.changed>600){displayTextures.set(stationKey(frame),frame.texture);continue}
      const next=1-state.index;
      gl.bindFramebuffer(gl.FRAMEBUFFER,smoothFramebuffer);gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,state.textures[next],0);gl.viewport(0,0,...size);
      gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,state.textures[state.index]);gl.activeTexture(gl.TEXTURE1);gl.bindTexture(gl.TEXTURE_2D,frame.texture);
      gl.uniform2f(gl.getUniformLocation(smoothProgram,'size'),...size);gl.uniform1f(gl.getUniformLocation(smoothProgram,'amount'),!state.initialized||now-state.changed>500?1:amount);gl.drawArrays(gl.TRIANGLES,0,3);
      state.initialized=true;state.index=next;displayTextures.set(stationKey(frame),state.textures[next]);
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER,null);gl.viewport(0,0,w,h);
    resetAttributes();gl.useProgram(imageProgram);
    gl.uniform2f(gl.getUniformLocation(imageProgram,'resolution'),w,h);
    gl.uniform2f(gl.getUniformLocation(imageProgram,'rotation'),yaw,pitch);
    gl.uniform1f(gl.getUniformLocation(imageProgram,'zoom'),zoom);
    gl.uniform1i(gl.getUniformLocation(imageProgram,'sourceMap'),1);gl.uniform1i(gl.getUniformLocation(imageProgram,'cameraVisibility'),2);
    gl.activeTexture(gl.TEXTURE1);gl.bindTexture(gl.TEXTURE_2D,sourceMapTexture);
    if(attribution&&uploadedSourceMap!==attribution.url){gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,attribution.width,attribution.height,0,gl.RGBA,gl.UNSIGNED_BYTE,attribution.data);uploadedSourceMap=attribution.url;}
    gl.activeTexture(gl.TEXTURE2);gl.bindTexture(gl.TEXTURE_2D,visibilityTexture);
    if(visibilityDirty){const pixels=new Uint8Array(256*256*4).fill(255);for(const [index,camera] of publicCameras){if(index<65536&&mutedSources.has(camera.source_id))pixels[index*4]=0;}gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,256,256,0,gl.RGBA,gl.UNSIGNED_BYTE,pixels);visibilityDirty=false;}
    const worldAttr=gl.getAttribLocation(imageProgram,'world'),rgbAttr=gl.getAttribLocation(imageProgram,'rgb'),uvAttr=gl.getAttribLocation(imageProgram,'uv');
    gl.uniform1i(gl.getUniformLocation(imageProgram,'textured'),1);gl.uniform1i(gl.getUniformLocation(imageProgram,'frame'),0);gl.activeTexture(gl.TEXTURE0);
    gl.disableVertexAttribArray(rgbAttr);gl.enableVertexAttribArray(worldAttr);gl.enableVertexAttribArray(uvAttr);
    if(browserLayers){canvas.dataset.composition=compositor.draw(w,h,yaw,pitch,zoom,frames,displayTextures,mutedSources);canvas.dataset.cameraLayers=String(frames.length);canvas.dataset.muteApplied=String(mutedSources.size>0);resetAttributes();gl.useProgram(imageProgram);gl.activeTexture(gl.TEXTURE0);gl.enableVertexAttribArray(worldAttr);}
    if(publicOnly&&!browserLayers){gl.enable(gl.BLEND);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);}
    for(const frame of browserLayers?[]:frames){const applyMute=mutedSources.size>0&&attribution?.url===frame.sourceMapUrl;gl.uniform1i(gl.getUniformLocation(imageProgram,'applyMute'),applyMute?1:0);canvas.dataset.muteApplied=String(applyMute);gl.bindBuffer(gl.ARRAY_BUFFER,frame.geometry.buffer);gl.vertexAttribPointer(worldAttr,3,gl.FLOAT,false,20,0);gl.vertexAttribPointer(uvAttr,2,gl.FLOAT,false,20,12);gl.bindTexture(gl.TEXTURE_2D,displayTextures.get(stationKey(frame))||frame.texture);gl.drawArrays(gl.TRIANGLES,0,frame.geometry.count)}
    if(publicOnly)gl.disable(gl.BLEND);
    gl.uniform1i(gl.getUniformLocation(imageProgram,'textured'),0);gl.disableVertexAttribArray(uvAttr);
    for(const layer of layers){
      gl.bindBuffer(gl.ARRAY_BUFFER,layer.buffer);
      for(const [name,offset] of [['world',0],['rgb',12]] as const){
        const a=gl.getAttribLocation(imageProgram,name);
        gl.enableVertexAttribArray(a);gl.vertexAttribPointer(a,3,gl.FLOAT,false,24,offset);
      }
      gl.drawArrays(gl.TRIANGLES,0,layer.count);
    }
    if(depthTest)gl.enable(gl.DEPTH_TEST);
    if(blend)gl.enable(gl.BLEND);
  };
  void (publicOnly?fetch(manifestUrl,{cache:'no-store'}).then(r=>r.json()).then(m=>m.igrf_url):Promise.resolve(`/gaia/api/igrf-maglat?format=f32-v2&year=${new Date().getUTCFullYear()}`)).then(url=>fetch(url)).then(r=>{if(!r.ok)throw new Error(`IGRF grid ${r.status}`);return r.arrayBuffer()}).then(buffer=>{const values=new Float32Array(buffer);if(values.length!==720*361)throw new Error(`unexpected IGRF grid length ${values.length}`);const vertices=igrfContourVertices(values);magneticVertices=vertices.length/2;gl.bindBuffer(gl.ARRAY_BUFFER,magneticBuffer);gl.bufferData(gl.ARRAY_BUFFER,vertices,gl.STATIC_DRAW)}).catch(console.error);
  const solarDirection=(time:number)=>{const jd=time/86400000+2440587.5,t=(jd-2451545)/36525,l0=(280.46646+t*(36000.76983+t*.0003032))*Math.PI/180,m=(357.52911+t*(35999.05029-.0001537*t))*Math.PI/180,lambda=l0+(1.914602-.004817*t-.000014*t*t)*Math.sin(m)*Math.PI/180+.019993*Math.sin(2*m)*Math.PI/180+.000289*Math.sin(3*m)*Math.PI/180,epsilon=(23.439291-.0130042*t)*Math.PI/180,decl=Math.asin(Math.sin(epsilon)*Math.sin(lambda)),ra=Math.atan2(Math.cos(epsilon)*Math.sin(lambda),Math.cos(lambda)),gmst=(280.46061837+360.98564736629*(jd-2451545)+.000387933*t*t-t*t*t/38710000)*Math.PI/180,lon=ra-gmst;return[Math.cos(decl)*Math.sin(lon),Math.sin(decl),Math.cos(decl)*Math.cos(lon)]};
  // Orientation for the sun-fixed, earth-rotating mode. Note the GLSL mat3
  // constructors above are column-major, so the view is
  // camX=cos(yaw)x-sin(yaw)z, camY=cos(pitch)y+sin(pitch)z', camZ=-sin(pitch)y+cos(pitch)z'.
  // pitch=-PI/2 puts the north pole at camera (0,0,1), the centre of the disc, so
  // the view looks straight down on the pole. yaw then spins the globe until the
  // subsolar meridian points to the top: the sun maps to (0,cos decl,sin decl),
  // straight up, and the planet rotates underneath it as the epoch advances.
  const sunUpOrientation=(sun:number[]):[number,number]=>[Math.atan2(sun[0],sun[2])+Math.PI,sunTilt];
  let lastEpochTime=performance.now();
  const draw=()=>{const dpr=Math.min(devicePixelRatio||1,2),w=Math.floor(canvas.clientWidth*dpr),h=Math.floor(canvas.clientHeight*dpr);if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h}gl.viewport(0,0,w,h);resetAttributes();gl.useProgram(program);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.enableVertexAttribArray(position);gl.vertexAttribPointer(position,2,gl.FLOAT,false,0,0);gl.uniform2f(resolution,w,h);gl.uniform1f(zoomLoc,zoom);const now=performance.now();const target=getEpochMillis();if(target<displayEpoch||Math.abs(target-displayEpoch)>600000)displayEpoch=target;else displayEpoch+=(target-displayEpoch)*(1-Math.exp(-Math.min(100,now-lastEpochTime)/65));lastEpochTime=now;const sun=solarDirection(displayEpoch);if(sunLock){const[lockedYaw,lockedPitch]=sunUpOrientation(sun);yaw=lockedYaw;pitch=lockedPitch}gl.uniform2f(rotation,yaw,pitch);gl.uniform3f(sunLoc,sun[0],sun[1],sun[2]);gl.drawArrays(gl.TRIANGLES,0,3);for(const [lineBuffer,count,color] of [[boundaryBuffer,boundaryVertices,[.52,.68,.72]],[magneticBuffer,magneticVertices,[.97,.48,1.]]] as const){
    if(!count)continue;resetAttributes();gl.useProgram(lineProgram);gl.bindBuffer(gl.ARRAY_BUFFER,lineBuffer);
    gl.enableVertexAttribArray(magneticPosition);gl.vertexAttribPointer(magneticPosition,2,gl.FLOAT,false,0,0);
    gl.uniform2f(gl.getUniformLocation(lineProgram,'resolution'),w,h);
    gl.uniform2f(gl.getUniformLocation(lineProgram,'rotation'),yaw,pitch);
    gl.uniform1f(gl.getUniformLocation(lineProgram,'zoom'),zoom);
    gl.uniform3f(gl.getUniformLocation(lineProgram,'lineColor'),...color);
    gl.drawArrays(gl.LINES,0,count);
  }drawLayers(w,h);measure();animation=requestAnimationFrame(draw)};draw();
  return()=>{cancelHold();canvas.removeEventListener('contextmenu',suppressNativeMenu);canvas.removeEventListener('selectstart',suppressNativeMenu);gl.deleteTexture(sourceMapTexture);gl.deleteTexture(visibilityTexture);window.removeEventListener('keydown',muteKey);perf?.remove();clearSmooth();gl.deleteFramebuffer(smoothFramebuffer);gl.deleteProgram(smoothProgram);tooltip.remove();canvas.removeEventListener('pointermove',hover);canvas.removeEventListener('pointerleave',hideTooltip);compositor.dispose();abort.abort();frameAbort.abort();clearInterval(frameTimer);canvas.removeEventListener('gaia-zoom',zoomControl);canvas.removeEventListener('gaia-sunlock',sunLockControl);for(const layer of layers)gl.deleteBuffer(layer.buffer);for(const g of geometryCache.values())gl.deleteBuffer(g.buffer);for(const t of textureCache.values())gl.deleteTexture(t);cancelAnimationFrame(animation);canvas.removeEventListener('pointerdown',pointerDown);canvas.removeEventListener('pointermove',pointerMove);canvas.removeEventListener('pointerup',pointerUp);canvas.removeEventListener('pointercancel',pointerUp);canvas.removeEventListener('lostpointercapture',pointerUp);canvas.removeEventListener('wheel',wheel)};
}
