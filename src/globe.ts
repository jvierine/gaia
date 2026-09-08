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
uniform sampler2D boundaries;
uniform sampler2D magneticLatitude;
uniform float magneticReady;

const float PI=3.141592653589793;
mat3 rotY(float a){float c=cos(a),s=sin(a);return mat3(c,0.,-s,0.,1.,0.,s,0.,c);}
mat3 rotX(float a){float c=cos(a),s=sin(a);return mat3(1.,0.,0.,0.,c,s,0.,-s,c);}
float line(float value,float count,float width){float q=abs(fract(value*count+.5)-.5);return 1.-smoothstep(.001*width,.004*width,q);}
float station(vec2 ll,vec2 target){vec2 d=vec2((ll.x-target.x)*cos(target.y),ll.y-target.y);return 1.-smoothstep(.011,.020,length(d));}
float hash(vec2 p){return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);}

void main(){
  vec2 p=(2.*gl_FragCoord.xy-resolution.xy)/min(resolution.x,resolution.y)/zoom;
  float r2=dot(p,p); vec3 bg=vec3(.003,.012,.022);
  float stars=step(.9975,hash(floor(gl_FragCoord.xy*.7)))*.34;
  if(r2>1.){gl_FragColor=vec4(bg+stars,1.);return;}
  vec3 n=normalize(vec3(p.x,p.y,sqrt(1.-r2)));
  n=rotY(rotation.x)*rotX(rotation.y)*n;
  float lat=asin(n.y),lon=atan(n.x,n.z); vec2 ll=vec2(lon,lat);
  float limb=pow(max(0.,sqrt(1.-r2)),.42);
  float grid=max(line((lat/PI)+.5,18.,.33),line((lon/(2.*PI))+.5,36.,.26));
  float geoEquator=1.-smoothstep(.0025,.006,abs(lat));
  vec2 mapUv=vec2(lon/(2.*PI)+.5,.5-lat/PI);
  float magneticDeg=texture2D(magneticLatitude,mapUv).r*180.-90.;
  float magneticDistance=abs(fract((magneticDeg+15.)/30.)-.5)*30.;
  float magneticContours=(1.-smoothstep(.30,.72,magneticDistance))*magneticReady;
  float magneticEquator=(1.-smoothstep(.30,.72,abs(magneticDeg)))*magneticReady;
  float borders=texture2D(boundaries,mapUv).r;
  float oceanNoise=.5+.5*sin(lon*4.+sin(lat*7.))*sin(lat*9.-lon*2.);
  vec3 color=mix(vec3(.010,.055,.085),vec3(.022,.13,.16),oceanNoise*.28);
  color+=grid*vec3(.08,.24,.29);
  color=mix(color,vec3(.16,.60,.72),geoEquator*.62);
  color=mix(color,vec3(.97,.48,1.),magneticContours*.86);
  color=mix(color,vec3(.97,.48,1.),magneticEquator*.86);
  color=mix(color,vec3(.64,.80,.84),borders*.82);
  float sites=0.;
  sites=max(sites,station(ll,vec2(.331,1.216))); sites=max(sites,station(ll,vec2(.356,1.184)));
  sites=max(sites,station(ll,vec2(.271,1.344))); sites=max(sites,station(ll,vec2(.465,1.176)));
  sites=max(sites,station(ll,vec2(.271,1.344))); 
  color=mix(color,vec3(.76,1.,.91),sites);
  float sunlight=dot(n,sunDirection);color*=.24+.84*smoothstep(-.04,.10,sunlight);
  float terminator=1.-smoothstep(.003,.012,abs(sunlight));color=mix(color,vec3(1.,.82,.20),terminator*.82);
  float sunMarker=1.-smoothstep(.020,.034,length(n-sunDirection));color=mix(color,vec3(1.,.94,.55),sunMarker);
  color*=limb; float rim=pow(1.-sqrt(1.-r2),7.);color+=rim*vec3(.10,.48,.54);
  gl_FragColor=vec4(color,1.);
}`;

function shader(gl: WebGLRenderingContext, type: number, source: string) {
  const result = gl.createShader(type)!; gl.shaderSource(result, source); gl.compileShader(result);
  if (!gl.getShaderParameter(result, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(result) || 'Shader failed');
  return result;
}

export function startGaiaGlobe(canvas: HTMLCanvasElement,getEpochMillis:()=>number) {
  const gl = canvas.getContext('webgl', { antialias: true }); if (!gl) return () => {};
  const program = gl.createProgram()!; gl.attachShader(program, shader(gl, gl.VERTEX_SHADER, VERTEX)); gl.attachShader(program, shader(gl, gl.FRAGMENT_SHADER, FRAGMENT)); gl.linkProgram(program); gl.useProgram(program);
  const buffer = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, buffer); gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1,3,-1,-1,3]), gl.STATIC_DRAW);
  const position = gl.getAttribLocation(program,'position'); gl.enableVertexAttribArray(position); gl.vertexAttribPointer(position,2,gl.FLOAT,false,0,0);
  const resolution=gl.getUniformLocation(program,'resolution'),rotation=gl.getUniformLocation(program,'rotation'),zoomLoc=gl.getUniformLocation(program,'zoom'),sunLoc=gl.getUniformLocation(program,'sunDirection');
  let yaw=-.35,pitch=-.45,zoom=.78,dragging=false,last=[0,0],animation=0;
  const pointerDown=(e:PointerEvent)=>{dragging=true;last=[e.clientX,e.clientY];canvas.setPointerCapture(e.pointerId)};
  const pointerMove=(e:PointerEvent)=>{if(!dragging)return;yaw-=(e.clientX-last[0])*.006;pitch=Math.max(-1.35,Math.min(1.35,pitch-(e.clientY-last[1])*.006));last=[e.clientX,e.clientY]};
  const pointerUp=()=>{dragging=false}; const wheel=(e:WheelEvent)=>{e.preventDefault();zoom=Math.max(.5,Math.min(1.25,zoom*Math.exp(-e.deltaY*.001)))};
  canvas.addEventListener('pointerdown',pointerDown);canvas.addEventListener('pointermove',pointerMove);canvas.addEventListener('pointerup',pointerUp);canvas.addEventListener('wheel',wheel,{passive:false});
  const boundaryTexture=gl.createTexture();gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,boundaryTexture);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array([0,0,0,0]));gl.uniform1i(gl.getUniformLocation(program,'boundaries'),0);
  void fetch('/gaia/world.geojson').then(async r=>(await r.json()) as {features:{geometry:{type:string;coordinates:unknown}}[]}).then(world=>{const map=document.createElement('canvas');map.width=2048;map.height=1024;const c=map.getContext('2d')!;c.strokeStyle='#fff';c.lineWidth=1.15;c.globalAlpha=.8;const ring=(points:number[][])=>{c.beginPath();let started=false,last=0;for(const p of points){const x=(p[0]+180)/360*map.width,y=(90-p[1])/180*map.height;if(!started||Math.abs(x-last)>map.width/2)c.moveTo(x,y);else c.lineTo(x,y);started=true;last=x}c.stroke()};for(const f of world.features){const g=f.geometry;if(g.type==='Polygon')for(const r of g.coordinates as number[][][])ring(r);if(g.type==='MultiPolygon')for(const p of g.coordinates as number[][][][])for(const r of p)ring(r)}gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,boundaryTexture);gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL,0);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,gl.RGBA,gl.UNSIGNED_BYTE,map);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.REPEAT);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE)}).catch(console.warn);
  const magneticTexture=gl.createTexture();gl.activeTexture(gl.TEXTURE1);gl.bindTexture(gl.TEXTURE_2D,magneticTexture);gl.texImage2D(gl.TEXTURE_2D,0,gl.LUMINANCE,1,1,0,gl.LUMINANCE,gl.UNSIGNED_BYTE,new Uint8Array([128]));gl.uniform1i(gl.getUniformLocation(program,'magneticLatitude'),1);
  const magneticReady=gl.getUniformLocation(program,'magneticReady');gl.uniform1f(magneticReady,0);
  void fetch('/gaia/api/igrf-maglat').then(r=>{if(!r.ok)throw new Error(`IGRF grid ${r.status}`);return r.arrayBuffer()}).then(buffer=>{const bytes=new Uint8Array(buffer);if(bytes.length!==360*181)throw new Error(`unexpected IGRF grid length ${bytes.length}`);gl.activeTexture(gl.TEXTURE1);gl.bindTexture(gl.TEXTURE_2D,magneticTexture);gl.pixelStorei(gl.UNPACK_ALIGNMENT,1);gl.texImage2D(gl.TEXTURE_2D,0,gl.LUMINANCE,360,181,0,gl.LUMINANCE,gl.UNSIGNED_BYTE,bytes);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.LINEAR);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.REPEAT);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);gl.useProgram(program);gl.uniform1f(magneticReady,1)}).catch(console.error);
  const solarDirection=(time:number)=>{const jd=time/86400000+2440587.5,t=(jd-2451545)/36525,l0=(280.46646+t*(36000.76983+t*.0003032))*Math.PI/180,m=(357.52911+t*(35999.05029-.0001537*t))*Math.PI/180,lambda=l0+(1.914602-.004817*t-.000014*t*t)*Math.sin(m)*Math.PI/180+.019993*Math.sin(2*m)*Math.PI/180+.000289*Math.sin(3*m)*Math.PI/180,epsilon=(23.439291-.0130042*t)*Math.PI/180,decl=Math.asin(Math.sin(epsilon)*Math.sin(lambda)),ra=Math.atan2(Math.cos(epsilon)*Math.sin(lambda),Math.cos(lambda)),gmst=(280.46061837+360.98564736629*(jd-2451545)+.000387933*t*t-t*t*t/38710000)*Math.PI/180,lon=ra-gmst;return[Math.cos(decl)*Math.sin(lon),Math.sin(decl),Math.cos(decl)*Math.cos(lon)]};
  const draw=()=>{const dpr=Math.min(devicePixelRatio||1,2),w=Math.floor(canvas.clientWidth*dpr),h=Math.floor(canvas.clientHeight*dpr);if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h}gl.viewport(0,0,w,h);gl.uniform2f(resolution,w,h);gl.uniform2f(rotation,yaw,pitch);gl.uniform1f(zoomLoc,zoom);const sun=solarDirection(getEpochMillis());gl.uniform3f(sunLoc,sun[0],sun[1],sun[2]);gl.drawArrays(gl.TRIANGLES,0,3);animation=requestAnimationFrame(draw)};draw();
  return()=>{cancelAnimationFrame(animation);canvas.removeEventListener('pointerdown',pointerDown);canvas.removeEventListener('pointermove',pointerMove);canvas.removeEventListener('pointerup',pointerUp);canvas.removeEventListener('wheel',wheel)};
}
