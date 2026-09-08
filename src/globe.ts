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

export function startGaiaGlobe(canvas: HTMLCanvasElement,getEpochMillis:()=>number) {
  const gl = canvas.getContext('webgl', { antialias: true }); if (!gl) return () => {};
  const program=makeProgram(gl,VERTEX,FRAGMENT),lineProgram=makeProgram(gl,LINE_VERTEX,LINE_FRAGMENT);gl.useProgram(program);
  const buffer = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, buffer); gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1,3,-1,-1,3]), gl.STATIC_DRAW);
  const position = gl.getAttribLocation(program,'position'); gl.enableVertexAttribArray(position); gl.vertexAttribPointer(position,2,gl.FLOAT,false,0,0);
  const resolution=gl.getUniformLocation(program,'resolution'),rotation=gl.getUniformLocation(program,'rotation'),zoomLoc=gl.getUniformLocation(program,'zoom'),sunLoc=gl.getUniformLocation(program,'sunDirection');
  let yaw=-.35,pitch=-.45,zoom=.78,dragging=false,last=[0,0],animation=0;
  const zoomControl=(event:Event)=>{const action=(event as CustomEvent<string>).detail;if(action==='reset'){zoom=.78}else{zoom=Math.max(.5,Math.min(8,zoom*(action==='in'?1.35:1/1.35)))}};
  canvas.addEventListener('gaia-zoom',zoomControl);
  const pointerDown=(e:PointerEvent)=>{dragging=true;last=[e.clientX,e.clientY];canvas.setPointerCapture(e.pointerId)};
  const pointerMove=(e:PointerEvent)=>{if(!dragging)return;yaw-=(e.clientX-last[0])*.006;pitch=Math.max(-1.35,Math.min(1.35,pitch-(e.clientY-last[1])*.006));last=[e.clientX,e.clientY]};
  const pointerUp=()=>{dragging=false}; const wheel=(e:WheelEvent)=>{e.preventDefault();zoom=Math.max(.5,Math.min(8,zoom*Math.exp(-e.deltaY*.001)))};
  canvas.addEventListener('pointerdown',pointerDown);canvas.addEventListener('pointermove',pointerMove);canvas.addEventListener('pointerup',pointerUp);canvas.addEventListener('wheel',wheel,{passive:false});
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
attribute vec3 world;attribute vec3 rgb;varying vec3 color;varying float visible;
uniform vec2 resolution;uniform vec2 rotation;uniform float zoom;
mat3 rotY(float a){float c=cos(a),s=sin(a);return mat3(c,0.,-s,0.,1.,0.,s,0.,c);}
mat3 rotX(float a){float c=cos(a),s=sin(a);return mat3(1.,0.,0.,0.,c,s,0.,-s,c);}
void main(){vec3 p=rotX(-rotation.y)*rotY(-rotation.x)*world;float side=min(resolution.x,resolution.y);gl_Position=vec4(p.xy*zoom*side/resolution,0.,1.);visible=p.z;color=rgb;gl_PointSize=3.;}`,`
precision highp float;varying vec3 color;varying float visible;void main(){if(visible<0.)discard;gl_FragColor=vec4(color,1.);}`);
  const layers:{buffer:WebGLBuffer;count:number;points:boolean}[]=[];
  const addLayer=(values:number[]|Float32Array,points:boolean)=>{const b=gl.createBuffer()!;gl.bindBuffer(gl.ARRAY_BUFFER,b);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array(values),gl.STATIC_DRAW);layers.push({buffer:b,count:values.length/6,points})};
  const abort=new AbortController();
  void fetch('/gaia/api/sources',{cache:'no-store',signal:abort.signal}).then(r=>r.json()).then(async (sources:{id:string;name:string;producer:string;calibrated:boolean;enabled:boolean;latitude_deg:number|null;longitude_deg:number|null}[])=>{
    const focus=sources.find(s=>s.enabled&&s.calibrated&&s.latitude_deg!==null&&s.longitude_deg!==null);if(focus){yaw=focus.longitude_deg!*Math.PI/180;pitch=-focus.latitude_deg!*Math.PI/180;}
    const sites:number[]=[];for(const s of sources){if(s.latitude_deg===null||s.longitude_deg===null)continue;const lat=s.latitude_deg*Math.PI/180,lon=s.longitude_deg*Math.PI/180;sites.push(Math.cos(lat)*Math.sin(lon),Math.sin(lat),Math.cos(lat)*Math.cos(lon),...(s.enabled?(s.calibrated?[.3,1,.7]:[1,.25,.3]):[.5,.5,.5]));}
    addLayer(sites,true);
    const queue=sources.filter(s=>s.enabled&&s.calibrated);
    await Promise.all([0,1].map(async()=>{while(queue.length&&!abort.signal.aborted){
      const s=queue.shift()!;
      try{
        const r=await fetch(`/gaia/api/sources/${encodeURIComponent(s.id)}/projection`,{headers:{Accept:'application/octet-stream'},cache:'no-store',signal:abort.signal});
        if(!r.ok)throw new Error(await r.text());
        const bytes=await r.arrayBuffer();if(abort.signal.aborted)return;
        addLayer(new Float32Array(bytes),false);
      }catch(e){if(!abort.signal.aborted)console.error(`Projection ${s.name}`,e)}
    }}));
  }).catch(console.error);
  // Final overlay pass: measured image colors are unlit and opaque, above both
  // the Earth's night shading and IGRF lines. The shader still hides the far side.
  const drawLayers=(w:number,h:number)=>{
    const depthTest=gl.isEnabled(gl.DEPTH_TEST),blend=gl.isEnabled(gl.BLEND);
    gl.disable(gl.DEPTH_TEST);gl.disable(gl.BLEND);
    gl.useProgram(imageProgram);
    gl.uniform2f(gl.getUniformLocation(imageProgram,'resolution'),w,h);
    gl.uniform2f(gl.getUniformLocation(imageProgram,'rotation'),yaw,pitch);
    gl.uniform1f(gl.getUniformLocation(imageProgram,'zoom'),zoom);
    for(const layer of [...layers.filter(l=>!l.points),...layers.filter(l=>l.points)]){
      gl.bindBuffer(gl.ARRAY_BUFFER,layer.buffer);
      for(const [name,offset] of [['world',0],['rgb',12]] as const){
        const a=gl.getAttribLocation(imageProgram,name);
        gl.enableVertexAttribArray(a);gl.vertexAttribPointer(a,3,gl.FLOAT,false,24,offset);
      }
      gl.drawArrays(layer.points?gl.POINTS:gl.TRIANGLES,0,layer.count);
    }
    if(depthTest)gl.enable(gl.DEPTH_TEST);
    if(blend)gl.enable(gl.BLEND);
  };
  void fetch(`/gaia/api/igrf-maglat?format=f32-v2&year=${new Date().getUTCFullYear()}`).then(r=>{if(!r.ok)throw new Error(`IGRF grid ${r.status}`);return r.arrayBuffer()}).then(buffer=>{const values=new Float32Array(buffer);if(values.length!==720*361)throw new Error(`unexpected IGRF grid length ${values.length}`);const vertices=igrfContourVertices(values);magneticVertices=vertices.length/2;gl.bindBuffer(gl.ARRAY_BUFFER,magneticBuffer);gl.bufferData(gl.ARRAY_BUFFER,vertices,gl.STATIC_DRAW)}).catch(console.error);
  const solarDirection=(time:number)=>{const jd=time/86400000+2440587.5,t=(jd-2451545)/36525,l0=(280.46646+t*(36000.76983+t*.0003032))*Math.PI/180,m=(357.52911+t*(35999.05029-.0001537*t))*Math.PI/180,lambda=l0+(1.914602-.004817*t-.000014*t*t)*Math.sin(m)*Math.PI/180+.019993*Math.sin(2*m)*Math.PI/180+.000289*Math.sin(3*m)*Math.PI/180,epsilon=(23.439291-.0130042*t)*Math.PI/180,decl=Math.asin(Math.sin(epsilon)*Math.sin(lambda)),ra=Math.atan2(Math.cos(epsilon)*Math.sin(lambda),Math.cos(lambda)),gmst=(280.46061837+360.98564736629*(jd-2451545)+.000387933*t*t-t*t*t/38710000)*Math.PI/180,lon=ra-gmst;return[Math.cos(decl)*Math.sin(lon),Math.sin(decl),Math.cos(decl)*Math.cos(lon)]};
  const draw=()=>{const dpr=Math.min(devicePixelRatio||1,2),w=Math.floor(canvas.clientWidth*dpr),h=Math.floor(canvas.clientHeight*dpr);if(canvas.width!==w||canvas.height!==h){canvas.width=w;canvas.height=h}gl.viewport(0,0,w,h);gl.useProgram(program);gl.bindBuffer(gl.ARRAY_BUFFER,buffer);gl.enableVertexAttribArray(position);gl.vertexAttribPointer(position,2,gl.FLOAT,false,0,0);gl.uniform2f(resolution,w,h);gl.uniform2f(rotation,yaw,pitch);gl.uniform1f(zoomLoc,zoom);const sun=solarDirection(getEpochMillis());gl.uniform3f(sunLoc,sun[0],sun[1],sun[2]);gl.drawArrays(gl.TRIANGLES,0,3);for(const [lineBuffer,count,color] of [[boundaryBuffer,boundaryVertices,[.52,.68,.72]],[magneticBuffer,magneticVertices,[.97,.48,1.]]] as const){
    if(!count)continue;gl.useProgram(lineProgram);gl.bindBuffer(gl.ARRAY_BUFFER,lineBuffer);
    gl.enableVertexAttribArray(magneticPosition);gl.vertexAttribPointer(magneticPosition,2,gl.FLOAT,false,0,0);
    gl.uniform2f(gl.getUniformLocation(lineProgram,'resolution'),w,h);
    gl.uniform2f(gl.getUniformLocation(lineProgram,'rotation'),yaw,pitch);
    gl.uniform1f(gl.getUniformLocation(lineProgram,'zoom'),zoom);
    gl.uniform3f(gl.getUniformLocation(lineProgram,'lineColor'),...color);
    gl.drawArrays(gl.LINES,0,count);
  }drawLayers(w,h);animation=requestAnimationFrame(draw)};draw();
  return()=>{abort.abort();for(const layer of layers)gl.deleteBuffer(layer.buffer);cancelAnimationFrame(animation);canvas.removeEventListener('pointerdown',pointerDown);canvas.removeEventListener('pointermove',pointerMove);canvas.removeEventListener('pointerup',pointerUp);canvas.removeEventListener('wheel',wheel)};
}
