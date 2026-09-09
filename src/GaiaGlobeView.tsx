import React,{type ReactNode,useEffect,useRef} from 'react';
import {startGaiaGlobe} from './globe';

type Props={
  className:string;
  getEpochMillis:()=>number;
  onLoading?:(loading:boolean)=>void;
  children?:ReactNode;
};

/** The single 3D stitched-atlas view used by both the public and admin shells. */
export default function GaiaGlobeView({className,getEpochMillis,onLoading=()=>{},children}:Props){
  const canvas=useRef<HTMLCanvasElement>(null),epoch=useRef(getEpochMillis),loading=useRef(onLoading);
  epoch.current=getEpochMillis;loading.current=onLoading;
  useEffect(()=>{
    if(!canvas.current)return;
    try{
      // Both shells deliberately use the published magnetic-weighted composite.
      // Keeping this fixed prevents the admin view from drifting to a different
      // per-camera alpha-overlay implementation.
      return startGaiaGlobe(canvas.current,()=>epoch.current(),value=>loading.current(value),true);
    }catch(error){
      loading.current(false);console.error('GAIA WebGL failed',error);canvas.current?.classList.add('webgl-failed');
    }
  },[]);
  const zoom=(detail:'in'|'out'|'reset')=>canvas.current?.dispatchEvent(new CustomEvent('gaia-zoom',{detail}));
  return <div className={className}>
    <canvas ref={canvas} className="gaia-globe-canvas" aria-label="Interactive WebGL Earth with auroral image coverage"/>
    <div className="gaia-globe-hint">Drag to rotate · Scroll or pinch to zoom · Hover or tap imagery for its camera</div>
    <div className="gaia-globe-tools" aria-label="Map controls"><button type="button" aria-label="Zoom in" onClick={()=>zoom('in')}>+</button><button type="button" aria-label="Zoom out" onClick={()=>zoom('out')}>−</button><button type="button" aria-label="Reset globe" onClick={()=>zoom('reset')}>◎</button></div>
    {children}
  </div>;
}
