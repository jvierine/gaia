// Shared normalized compositor. No winner-takes-all colour fallback.
export type CameraLayer={geometry:{buffer:WebGLBuffer;count:number};texture:WebGLTexture;sourceId?:string;weightScale?:number;order:number;at:string};
export function cameraCompositor(gl:WebGLRenderingContext,forcePortable=false){
  const attributeCount=gl.getParameter(gl.MAX_VERTEX_ATTRIBS) as number;
  const resetAttributes=()=>{for(let i=0;i<attributeCount;i++)gl.disableVertexAttribArray(i)};
  const audit=(stage:string)=>{if(new URLSearchParams(location.search).has('perf')){const error=gl.getError();if(error!==gl.NO_ERROR)console.warn(`Camera compositor ${stage}: WebGL ${error}`)}};
  const program=(v:string,f:string)=>{const p=gl.createProgram()!;for(const [type,source] of [[gl.VERTEX_SHADER,v],[gl.FRAGMENT_SHADER,f]] as const){const s=gl.createShader(type)!;gl.shaderSource(s,source);gl.compileShader(s);if(!gl.getShaderParameter(s,gl.COMPILE_STATUS))throw Error(gl.getShaderInfoLog(s)||'Camera shader');gl.attachShader(p,s);gl.deleteShader(s)}gl.linkProgram(p);if(!gl.getProgramParameter(p,gl.LINK_STATUS))throw Error(gl.getProgramInfoLog(p)||'Camera program');return p};
  const vertex=`attribute vec3 world;attribute vec2 uv;attribute float weight;uniform vec2 resolution;uniform vec2 rotation;uniform float zoom;uniform float scale;varying vec2 tc;varying float w;varying float visible;
mat3 ry(float a){float c=cos(a),s=sin(a);return mat3(c,0.,-s,0.,1.,0.,s,0.,c);}mat3 rx(float a){float c=cos(a),s=sin(a);return mat3(1.,0.,0.,0.,c,s,0.,-s,c);}
void main(){vec3 p=rx(-rotation.y)*ry(-rotation.x)*world;w=weight*scale;gl_Position=vec4(p.xy*zoom*min(resolution.x,resolution.y)/resolution,1.-2.*clamp(w,0.000001,0.999999),1.);tc=uv;visible=p.z;}`;
  const mesh=program(vertex,`precision highp float;varying vec2 tc;varying float w;varying float visible;uniform vec2 resolution;uniform sampler2D image;uniform sampler2D previousColor;uniform sampler2D previousWeight;uniform int mode;uniform vec3 code;
float decode(vec3 c){float n=dot(floor(c*255.+.5),vec3(65536.,256.,1.));return n<.5?0.:exp2(n/100000.-128.);}
vec3 encode(float n){float v=floor((log2(n)+128.)*100000.+.5);return vec3(floor(v/65536.),mod(floor(v/256.),256.),mod(v,256.))/255.;}
void main(){if(visible<0.||w<=0.)discard;vec2 screen=gl_FragCoord.xy/resolution;vec3 rgb=texture2D(image,tc).rgb;
if(mode==1){float scale=exp2(texture2D(previousWeight,screen).r*144.-128.);float a=w/scale;gl_FragColor=vec4(rgb*a,a);}
else if(mode==2){gl_FragColor=vec4(code,1.);}
else if(mode==3){gl_FragColor=vec4(vec3(clamp((log2(w)+128.)/144.,0.,1.)),1.);}
else{float old=decode(texture2D(previousWeight,screen).rgb),total=old+w;if(mode==4)gl_FragColor=vec4(encode(total),1.);else gl_FragColor=vec4((texture2D(previousColor,screen).rgb*old+rgb*w)/total,1.);}}`);
  const resolve=program(`attribute vec2 position;void main(){gl_Position=vec4(position,0.,1.);}`,`precision highp float;uniform vec2 resolution;uniform sampler2D sums;uniform bool normalize;void main(){vec4 c=texture2D(sums,gl_FragCoord.xy/resolution);gl_FragColor=normalize?(c.a>0.?vec4(c.rgb/c.a,1.):vec4(0.)):c;}`);
  const quad=gl.createBuffer()!;gl.bindBuffer(gl.ARRAY_BUFFER,quad);gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([-1,-1,3,-1,-1,3]),gl.STATIC_DRAW);
  const half=gl.getExtension('OES_texture_half_float'),color=gl.getExtension('EXT_color_buffer_half_float'),maxBlend=gl.getExtension('EXT_blend_minmax');
  const fbo=gl.createFramebuffer()!,depth=gl.createRenderbuffer()!;
  const dummy=gl.createTexture()!;gl.bindTexture(gl.TEXTURE_2D,dummy);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,1,1,0,gl.RGBA,gl.UNSIGNED_BYTE,new Uint8Array(4));
  const sum=gl.createTexture()!,pick=gl.createTexture()!,result=gl.createTexture()!,colors=[gl.createTexture()!,gl.createTexture()!],weights=[gl.createTexture()!,gl.createTexture()!];
  let size=[0,0],floating=!forcePortable&&!!(half&&color&&maxBlend),disposed=false,lastKey='';const ids=new WeakMap<object,number>();let nextId=1;const id=(o:object)=>{let v=ids.get(o);if(!v){v=nextId++;ids.set(o,v)}return v};
  const target=(texture:WebGLTexture|null)=>{gl.bindFramebuffer(gl.FRAMEBUFFER,texture?fbo:null);if(texture)gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,texture,0)};
  const allocate=(w:number,h:number)=>{if(size[0]===w&&size[1]===h)return;size=[w,h];lastKey='';for(const t of [sum,pick,result,...colors,...weights]){gl.bindTexture(gl.TEXTURE_2D,t);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MIN_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_MAG_FILTER,gl.NEAREST);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_S,gl.CLAMP_TO_EDGE);gl.texParameteri(gl.TEXTURE_2D,gl.TEXTURE_WRAP_T,gl.CLAMP_TO_EDGE);gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,w,h,0,gl.RGBA,t===sum&&floating?half!.HALF_FLOAT_OES:gl.UNSIGNED_BYTE,null)}gl.bindRenderbuffer(gl.RENDERBUFFER,depth);gl.renderbufferStorage(gl.RENDERBUFFER,gl.DEPTH_COMPONENT16,w,h);target(sum);gl.framebufferRenderbuffer(gl.FRAMEBUFFER,gl.DEPTH_ATTACHMENT,gl.RENDERBUFFER,depth);floating=floating&&gl.checkFramebufferStatus(gl.FRAMEBUFFER)===gl.FRAMEBUFFER_COMPLETE;target(null)};
  const blit=(texture:WebGLTexture,w:number,h:number,normalize=false)=>{resetAttributes();gl.useProgram(resolve);gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,texture);gl.uniform1i(gl.getUniformLocation(resolve,'sums'),0);gl.uniform1i(gl.getUniformLocation(resolve,'normalize'),normalize?1:0);gl.uniform2f(gl.getUniformLocation(resolve,'resolution'),w,h);gl.bindBuffer(gl.ARRAY_BUFFER,quad);const a=gl.getAttribLocation(resolve,'position');gl.enableVertexAttribArray(a);gl.vertexAttribPointer(a,2,gl.FLOAT,false,0,0);gl.drawArrays(gl.TRIANGLES,0,3)};
  const bind=(w:number,h:number,yaw:number,pitch:number,zoom:number)=>{resetAttributes();gl.useProgram(mesh);gl.uniform2f(gl.getUniformLocation(mesh,'resolution'),w,h);gl.uniform2f(gl.getUniformLocation(mesh,'rotation'),yaw,pitch);gl.uniform1f(gl.getUniformLocation(mesh,'zoom'),zoom);gl.uniform1i(gl.getUniformLocation(mesh,'image'),0);gl.uniform1i(gl.getUniformLocation(mesh,'previousColor'),1);gl.uniform1i(gl.getUniformLocation(mesh,'previousWeight'),2);for(const unit of [gl.TEXTURE1,gl.TEXTURE2]){gl.activeTexture(unit);gl.bindTexture(gl.TEXTURE_2D,dummy)}gl.activeTexture(gl.TEXTURE0)};
  const render=(frames:CameraLayer[],textures:Map<number,WebGLTexture>,muted:Set<string>,mode:number)=>{gl.uniform1i(gl.getUniformLocation(mesh,'mode'),mode);for(const frame of frames){if(frame.sourceId&&muted.has(frame.sourceId))continue;gl.bindBuffer(gl.ARRAY_BUFFER,frame.geometry.buffer);for(const [name,n,offset] of [['world',3,0],['uv',2,12],['weight',1,20]] as const){const a=gl.getAttribLocation(mesh,name);gl.enableVertexAttribArray(a);gl.vertexAttribPointer(a,n,gl.FLOAT,false,24,offset)}gl.uniform1f(gl.getUniformLocation(mesh,'scale'),frame.weightScale??1);const code=frame.order+1;gl.uniform3f(gl.getUniformLocation(mesh,'code'),((code>>16)&255)/255,((code>>8)&255)/255,(code&255)/255);gl.activeTexture(gl.TEXTURE0);gl.bindTexture(gl.TEXTURE_2D,textures.get(frame.order)||frame.texture);gl.drawArrays(gl.TRIANGLES,0,frame.geometry.count)}};
  return {
    draw(w:number,h:number,yaw:number,pitch:number,zoom:number,frames:CameraLayer[],textures:Map<number,WebGLTexture>,muted:Set<string>){
      audit('before');allocate(w,h);audit('allocation');gl.viewport(0,0,w,h);gl.disable(gl.BLEND);gl.disable(gl.DEPTH_TEST);gl.clearColor(0,0,0,0);
      const key=[w,h,yaw,pitch,zoom,...frames.filter(f=>!muted.has(f.sourceId||'')).map(f=>`${id(f.geometry.buffer)}:${id(textures.get(f.order)||f.texture)}:${f.weightScale}`)].join(',');
      if(key!==lastKey){
        if(floating){
          // Common per-pixel scaling cancels in normalization and prevents tiny
          // lone-camera mask/daylight weights underflowing in half-float storage.
          target(weights[0]);gl.clear(gl.COLOR_BUFFER_BIT);bind(w,h,yaw,pitch,zoom);gl.enable(gl.BLEND);gl.blendEquation(maxBlend!.MAX_EXT);gl.blendFunc(gl.ONE,gl.ONE);render(frames,textures,muted,3);gl.blendEquation(gl.FUNC_ADD);audit('maximum weight');
          target(sum);gl.clear(gl.COLOR_BUFFER_BIT);gl.activeTexture(gl.TEXTURE2);gl.bindTexture(gl.TEXTURE_2D,weights[0]);render(frames,textures,muted,1);audit('accumulate');gl.disable(gl.BLEND);target(result);blit(sum,w,h,true);audit('normalize');
        }else{
          // Same normalized running mean, with separately packed log weights.
          // Portable RGBA8 colour rounding differs, never the weighting rules.
          let previous=0;for(const t of [colors[0],weights[0]]){target(t);gl.clear(gl.COLOR_BUFFER_BIT)}
          for(const frame of frames){if(muted.has(frame.sourceId||''))continue;const next=1-previous;
            for(const [dest,source,mode] of [[colors[next],colors[previous],5],[weights[next],weights[previous],4]] as const){target(dest);blit(source,w,h);bind(w,h,yaw,pitch,zoom);gl.activeTexture(gl.TEXTURE1);gl.bindTexture(gl.TEXTURE_2D,colors[previous]);gl.activeTexture(gl.TEXTURE2);gl.bindTexture(gl.TEXTURE_2D,weights[previous]);render([frame],textures,muted,mode)}previous=next;
          }target(result);blit(colors[previous],w,h);
        }lastKey=key;
      }
      target(null);gl.enable(gl.BLEND);gl.blendEquation(gl.FUNC_ADD);gl.blendFunc(gl.SRC_ALPHA,gl.ONE_MINUS_SRC_ALPHA);blit(result,w,h);gl.disable(gl.BLEND);return floating?'magnetic-weighted GPU blend':'normalized GPU blend (portable)';
    },
    pick(x:number,y:number,w:number,h:number,yaw:number,pitch:number,zoom:number,frames:CameraLayer[],muted:Set<string>){if(disposed||!frames.length)return undefined;allocate(w,h);target(pick);gl.viewport(0,0,w,h);gl.disable(gl.BLEND);gl.enable(gl.DEPTH_TEST);gl.depthMask(true);gl.depthFunc(gl.LEQUAL);gl.clearColor(0,0,0,0);gl.clear(gl.COLOR_BUFFER_BIT|gl.DEPTH_BUFFER_BIT);bind(w,h,yaw,pitch,zoom);render(frames,new Map(),muted,2);const p=new Uint8Array(4);gl.readPixels(Math.floor(x),Math.floor(y),1,1,gl.RGBA,gl.UNSIGNED_BYTE,p);target(null);gl.disable(gl.DEPTH_TEST);const order=((p[0]<<16)|(p[1]<<8)|p[2])-1;return frames.find(f=>f.order===order)?.sourceId;},
    dispose(){disposed=true;gl.deleteProgram(mesh);gl.deleteProgram(resolve);gl.deleteBuffer(quad);gl.deleteFramebuffer(fbo);for(const t of [dummy,sum,pick,result,...colors,...weights])gl.deleteTexture(t);gl.deleteRenderbuffer(depth)}
  };
}
