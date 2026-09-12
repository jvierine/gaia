import React,{type ReactNode,useEffect,useRef,useState} from 'react';
import {startGaiaGlobe} from './globe';

type Props={
  className:string;
  getEpochMillis:()=>number;
  onLoading?:(loading:boolean)=>void;
  /** Drive the orientation from the sun-earth line, sun up, planet rotating underneath. */
  sunLock?:boolean;
  /** Set false to host the zoom controls outside the globe, e.g. in a side pane. */
  showTools?:boolean;
  /** Filled with the zoom action so a parent can drive it from its own controls. */
  zoomRef?:{current:((action:'in'|'out'|'reset')=>void)|null};
  children?:ReactNode;
};

/** The single 3D stitched-atlas view used by both the public and admin shells. */
export default function GaiaGlobeView({className,getEpochMillis,onLoading=()=>{},sunLock=false,showTools=true,zoomRef,children}:Props){
  const [buffer,setBuffer]=useState({active:false,done:0,total:0,failed:0,message:''});
  const canvas=useRef<HTMLCanvasElement>(null),epoch=useRef(getEpochMillis),loading=useRef(onLoading);
  epoch.current=getEpochMillis;loading.current=onLoading;
  useEffect(()=>{
    if(!canvas.current)return;
    const element=canvas.current;
    const progress=(event:Event)=>setBuffer({...{message:''},...(event as CustomEvent).detail});
    element.addEventListener('gaia-buffer-progress',progress);
    let stop:(()=>void)|undefined;
    try{
      // Both shells deliberately use the published magnetic-weighted composite.
      // Keeping this fixed prevents the admin view from drifting to a different
      // per-camera alpha-overlay implementation.
      stop=startGaiaGlobe(canvas.current,()=>epoch.current(),value=>loading.current(value),true);
    }catch(error){
      loading.current(false);console.error('GAIA WebGL failed',error);canvas.current?.classList.add('webgl-failed');
    }
    return()=>{element.removeEventListener('gaia-buffer-progress',progress);stop?.()};
  },[]);
  useEffect(()=>{canvas.current?.dispatchEvent(new CustomEvent('gaia-sunlock',{detail:sunLock}))},[sunLock]);
  const zoom=(detail:'in'|'out'|'reset')=>canvas.current?.dispatchEvent(new CustomEvent('gaia-zoom',{detail}));
  if(zoomRef)zoomRef.current=zoom;
  return <div className={className}>
    <canvas ref={canvas} className="gaia-globe-canvas" style={{WebkitTouchCallout:'none',WebkitUserSelect:'none',userSelect:'none',touchAction:'none'}} aria-label="Interactive WebGL Earth with auroral image coverage"/>
    <div className="gaia-globe-hint" style={{WebkitTouchCallout:'none',WebkitUserSelect:'none',userSelect:'none'}}>Drag to rotate · Pinch to zoom · Hold a station or press M to mute/show</div>
    {showTools&&<div className="gaia-globe-tools" aria-label="Map controls"><button type="button" aria-label="Zoom in" onClick={()=>zoom('in')}>+</button><button type="button" aria-label="Zoom out" onClick={()=>zoom('out')}>−</button><button type="button" aria-label="Reset globe" onClick={()=>zoom('reset')}>◎</button></div>}
    {(buffer.active||buffer.failed>0)&&<div role="status" style={{position:'absolute',zIndex:30,left:'50%',top:70,transform:'translateX(-50%)',width:'min(360px, calc(100% - 24px))',padding:12,borderRadius:10,background:'#101d29f2',color:'#fff',boxShadow:'0 2px 12px #0008',fontSize:14}}>
      <div>{buffer.active?'Buffering camera images…':buffer.message||`${buffer.failed} camera images unavailable; omitted from this frame.`}</div>
      {buffer.active&&<><progress aria-label="Camera image buffering" max={buffer.total||1} value={buffer.total?buffer.done:undefined} style={{width:'100%',height:16,accentColor:'#70d9ac'}}/><div>{buffer.total?`${buffer.done} / ${buffer.total} cameras checked · ${Math.round(100*buffer.done/buffer.total)}%`:buffer.message||'Preparing camera images…'}</div><button type="button" onClick={()=>{window.dispatchEvent(new Event('gaia-pause-playback'));canvas.current?.dispatchEvent(new Event('gaia-cancel-buffer'))}} style={{marginTop:8}}>Cancel playback</button></>}
    </div>}
    {children}
  </div>;
}
