import React,{useEffect,useRef,useState} from 'react';
import './viewer-settings.css';
export default function ViewerSettings({normalize,onNormalize}:{normalize:boolean;onNormalize:(value:boolean)=>void}){
  const [open,setOpen]=useState(false);const dialog=useRef<HTMLDialogElement>(null);
  useEffect(()=>{if(open&&!dialog.current?.open)dialog.current?.showModal()},[open]);
  return <><button type="button" className="gaia-settings-button" aria-label="Viewer settings" title="Viewer settings" onClick={()=>setOpen(true)}>
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><path d="m9 3 1-1h4l1 1 .4 2 1.7 1 2-.7 1.4 1.4 1 1.7-.7 2 1 1.6-.1 2-1.9 1-1 1.8.4 2-1.7 1.2-2-.6-1.7 1-.6 2H10l-.6-2-1.7-1-2 .6L4 19.8l.4-2-1-1.8-1.9-1-.1-2 1-1.6-.7-2 1-1.7L4.1 6l2 .7 1.7-1L9 3Z"/><circle cx="12" cy="12" r="3.5"/></svg>
  </button><dialog ref={dialog} className="gaia-settings-dialog" aria-labelledby="gaia-settings-heading" onClose={()=>setOpen(false)} onClick={e=>{if(e.target===dialog.current)dialog.current?.close()}}>
    <section onClick={e=>e.stopPropagation()}><header><h2 id="gaia-settings-heading">Viewer settings</h2><button type="button" aria-label="Close settings" onClick={()=>dialog.current?.close()}>×</button></header>
    <label><input type="checkbox" checked={normalize} onChange={e=>onNormalize(e.target.checked)}/><span>Normalize camera brightness</span></label>
    <p>Brighten dim cameras before blending them on the globe. Applies to live view and playback.</p><p className="gaia-settings-note">Display only. May amplify noise; original images, calibration and blending weights are unchanged.</p>
    </section></dialog></>;
}
