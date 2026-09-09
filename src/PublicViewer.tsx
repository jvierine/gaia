import React,{useEffect,useRef,useState} from 'react';
import {startGaiaGlobe} from './globe';
type Manifest={generated_utc:string;images:{at:string}[];credits:{name:string;website?:string;acknowledgement:string;copyright:string}[]};
export default function PublicViewer(){
  const canvas=useRef<HTMLCanvasElement>(null),epoch=useRef(Date.now()),loading=useRef(false);
  const [manifest,setManifest]=useState<Manifest>(),[index,setIndex]=useState(-1),[play,setPlay]=useState(false),[credits,setCredits]=useState(false),[error,setError]=useState('');
  useEffect(()=>{const controller=new AbortController();const refresh=()=>fetch('/gaia/public/manifest.json',{cache:'no-store',signal:controller.signal}).then(r=>{if(!r.ok)throw Error('Published images unavailable');return r.json()}).then(m=>{setManifest(m);setError('')}).catch(e=>{if(!controller.signal.aborted)setError(String(e.message))});void refresh();const timer=setInterval(refresh,60000);return()=>{controller.abort();clearInterval(timer)}},[]);
  useEffect(()=>canvas.current?startGaiaGlobe(canvas.current,()=>epoch.current,b=>{loading.current=b},true):undefined,[]);
  useEffect(()=>{if(manifest?.images.length)epoch.current=Date.parse(manifest.images[index<0?manifest.images.length-1:Math.min(index,manifest.images.length-1)].at)},[manifest,index]);
  useEffect(()=>{if(!play||!manifest)return;const timer=setInterval(()=>{if(!loading.current)setIndex(i=>(i+1)%manifest.images.length)},250);return()=>clearInterval(timer)},[play,manifest]);
  const frames=manifest?.images||[],at=frames[index<0?frames.length-1:Math.min(index,frames.length-1)]?.at;
  return <main style={{height:'100dvh',display:'flex',flexDirection:'column',background:'#01060b',color:'#c9eee2'}}>
    <div style={{position:'relative',flex:1,minHeight:0}}><canvas ref={canvas} style={{width:'100%',height:'100%',touchAction:'none'}}/><button aria-label="Credits" onClick={()=>setCredits(!credits)} style={{position:'absolute',left:16,top:16}}>ⓘ</button>
    {credits&&<section style={{position:'absolute',left:16,top:52,maxHeight:'75vh',overflow:'auto',maxWidth:480,background:'#08212a',padding:20}}><img src="/gaia/uit-logo-white.png" alt="UiT" width={120}/><h2>GAIA Data Center</h2><p>Juha Vierinen · Björn Gustavsson</p><p>Images retain their producers’ copyright. Display-only composites at 100 km; no synthetic aurora.</p>{manifest?.credits.map((c,i)=><p key={i}><a href={c.website||undefined} target="_blank" rel="noreferrer">{c.name}</a><br/>{c.acknowledgement}<br/>{c.copyright}</p>)}</section>}</div>
    <footer style={{padding:14,display:'flex',gap:12,alignItems:'center',flexWrap:'wrap'}}><button disabled={!frames.length} onClick={()=>{if(!play&&index<0)setIndex(0);setPlay(!play)}}>{play?'Pause':'Play'}</button><button onClick={()=>{setPlay(false);setIndex(-1)}}>Latest</button><input aria-label="Observation time" style={{flex:1,minWidth:120}} type="range" min={0} max={Math.max(0,frames.length-1)} value={index<0?Math.max(0,frames.length-1):index} onChange={e=>setIndex(Number(e.target.value))}/><time>{at?new Date(at).toISOString().replace('T',' ').replace('.000Z',' UTC'):'Waiting for publication'}</time>{error&&<span role="alert">{error}</span>}</footer>
  </main>
}
