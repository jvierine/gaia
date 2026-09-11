import {liveCutoff,readyFrames,LIVE_DELAY_MINUTES} from './live-time';
import React,{useEffect,useRef,useState} from 'react';
import './public-viewer.css';
import GaiaLogo from './GaiaLogo';
import GaiaGlobeView from './GaiaGlobeView';
import {manifestUrl,archiveMode} from './public-manifest';

type LensModel={calibration_id:string;created_utc:string;valid_from_utc?:string|null;valid_to_utc?:string|null;method:string;residual_px?:number|null;format:string;sha256:string;url:string};
type CameraLens={source_id:string;name:string;producer:string;latitude_deg?:number|null;longitude_deg?:number|null;models:LensModel[]};
type Camera={source_id:string;name:string;producer:string;institution:string;website_url:string;latitude_deg:number|null;longitude_deg:number|null;acknowledgement:string;copyright:string;calibrated:boolean;map_index:number|null};
type Manifest={generated_utc:string;images:{at:string}[];cameras?:Camera[];lens_models?:CameraLens[]};
const date=(value?:string|null)=>value?new Date(value).toISOString().slice(0,10):'open';
const location=(camera:Camera)=>camera.latitude_deg==null||camera.longitude_deg==null?'Location unavailable':`${Math.abs(camera.latitude_deg).toFixed(3)}°${camera.latitude_deg>=0?'N':'S'}, ${Math.abs(camera.longitude_deg).toFixed(3)}°${camera.longitude_deg>=0?'E':'W'}`;

export default function PublicViewer(){
  const epoch=useRef(liveCutoff()),loading=useRef(false);
  const [manifest,setManifest]=useState<Manifest>(),[index,setIndex]=useState(-1),[play,setPlay]=useState(false),[credits,setCredits]=useState(false),[error,setError]=useState(''),[sunLock,setSunLock]=useState(false);
  useEffect(()=>{const controller=new AbortController();const refresh=()=>fetch(manifestUrl,{cache:'no-store',signal:controller.signal}).then(r=>{if(!r.ok)throw Error('Published images unavailable');return r.json()}).then(m=>{setManifest({...m,images:readyFrames(m.images)});setError('')}).catch(e=>{if(!controller.signal.aborted)setError(String(e.message))});void refresh();const timer=setInterval(refresh,60000);return()=>{controller.abort();clearInterval(timer)}},[]);
  useEffect(()=>{if(manifest?.images.length)epoch.current=Date.parse(manifest.images[index<0?manifest.images.length-1:Math.min(index,manifest.images.length-1)].at)},[manifest,index]);
  useEffect(()=>{if(!play||!manifest?.images.length)return;const timer=setInterval(()=>{if(!loading.current)setIndex(i=>(i+1)%manifest.images.length)},250);return()=>clearInterval(timer)},[play,manifest]);
  const frames=manifest?.images||[],at=frames[index<0?frames.length-1:Math.min(index,frames.length-1)]?.at;
  return <main className="gaia-public">
    <header className="public-header"><div className="public-brand"><GaiaLogo/><div><h1>GAIA <span>Data Center</span></h1><p>Global Auroral Image Archive</p></div></div><button aria-expanded={credits} onClick={()=>setCredits(!credits)}>ⓘ <span>Info &amp; credits</span></button></header>
    <GaiaGlobeView className="public-globe" getEpochMillis={()=>epoch.current} onLoading={value=>{loading.current=value}} sunLock={sunLock}>
    {credits&&<section className="public-info" role="dialog" aria-label="Information and credits">
      <div className="public-info-heading"><h2>About GAIA</h2><button onClick={()=>setCredits(false)} aria-label="Close information">✕</button></div>
      <p>GAIA compiles low-resolution, geographically projected views of aurora from publicly available data sources. GAIA <strong>does not provide or redistribute the original high-resolution camera images</strong>. Obtain originals directly from the originating provider using the camera links below or by clicking projected imagery on the globe.</p>
      <p>Hover over a projected image to see its institution and station location. Click or tap it to open the originating provider. Gold dots mark contributing camera stations.</p>
      <p>GAIA was inspired in part by the <a href="https://data.phys.ucalgary.ca/" target="_blank" rel="noreferrer">University of Calgary THEMIS All-Sky Imager array</a>, whose continent-scale observations have been enormously influential in auroral research. GAIA is an independent project.</p>
      <p>Drag to rotate the globe. Scroll or pinch to zoom. Press Play to explore recent auroral activity, or Latest to see the newest ready images. Live viewing uses a {LIVE_DELAY_MINUTES}-minute delay to allow cameras to arrive and processing to finish. Tick <strong>Sun up</strong> to hold the sun upwards while Earth rotates. Drag vertically to adjust the viewing tilt.</p>
      <h2>Camera lens models</h2>
      <p>Downloadable calibrations are the exact AIDA/WISC HDF5 files used to place camera pixels on the globe. Azimuth is degrees clockwise from geographic north; elevation is degrees above the horizon. Image coordinates are zero-based raw pixel centres.</p>
      <p>In Python, read <code>/wisc_optpar_with_optmod</code> and the root <code>image_width</code> and <code>image_height</code> attributes with <code>h5py</code>, then pass them with <code>(x, y)</code> to <a href="https://github.com/jvierine/widefield-star-calibrator/blob/main/wisc_lens.py" target="_blank" rel="noreferrer"><code>pixel_to_az_el</code></a>. For dated images, select the model whose half-open validity interval contains the observation time. Verify the downloaded file against its SHA-256 value.</p>
      {manifest?.lens_models?.map(camera=><details className="lens-camera" key={camera.source_id}><summary>{camera.name} <span>{camera.models.length} model{camera.models.length===1?'':'s'}</span></summary><p>{camera.producer}{camera.latitude_deg!=null&&camera.longitude_deg!=null?` · ${camera.latitude_deg.toFixed(3)}°, ${camera.longitude_deg.toFixed(3)}°`:''}</p><ul>{camera.models.map(model=><li key={model.calibration_id}><a href={model.url} download>{date(model.valid_from_utc)}–{date(model.valid_to_utc)} · Download HDF5</a>{model.residual_px!=null?` · ${model.residual_px.toFixed(3)} px RMS`:''}<br/><code>SHA-256 {model.sha256}</code></li>)}</ul></details>)}
      <h2>GAIA aggregation and projection service</h2>
      <p>GAIA is an independent image aggregation and geographic projection service developed by Juha Vierinen and Björn Gustavsson. Contributing camera networks remain independently operated, and their images remain the copyright of their respective producers.</p>
      <h2>Camera providers &amp; credits</h2>
      <p>Every listed camera links to its originating provider. All images remain the copyright of their respective producers.</p>
      <div className="camera-credits">{manifest?.cameras?.map(camera=><details key={camera.source_id}><summary><a href={camera.website_url} target="_blank" rel="noreferrer">{camera.name} ↗</a></summary><p><strong>{camera.institution}</strong><br/>{location(camera)}<br/>{camera.acknowledgement}<br/>{camera.copyright}</p></details>)}</div>
    </section>}</GaiaGlobeView>
    <section className="public-playback" aria-label="Aurora playback"><div className="public-playback-row"><button className="public-play" disabled={!frames.length} onClick={()=>{if(!play&&index<0)setIndex(0);setPlay(!play)}}>{play?'❚❚ Pause':'▶ Play'}</button><label className="sun-lock" title="Hold the sun-earth line fixed with the sun upwards, drag vertically to tilt towards the nightside"><input type="checkbox" checked={sunLock} onChange={event=>setSunLock(event.target.checked)}/><span><strong>Sun up</strong><small>rotate vs. sun–earth line</small></span></label><button disabled={!frames.length} onClick={()=>{setPlay(false);setIndex(-1)}} title={`Newest published frame at least ${LIVE_DELAY_MINUTES} minutes behind now`}>Latest (−{LIVE_DELAY_MINUTES} min)</button><time>{at?new Date(at).toISOString().replace('T',' ').replace('.000Z',' UTC'):'Waiting for publication'}</time></div><input aria-label="Observation time" disabled={!frames.length} type="range" min={0} max={Math.max(0,frames.length-1)} value={index<0?Math.max(0,frames.length-1):Math.min(index,frames.length-1)} onChange={e=>setIndex(Number(e.target.value))}/><div className="public-timeline-labels"><span>{frames.length?new Date(frames[0].at).toISOString().slice(0,16).replace('T',' '):'No frames'} UTC</span><span>{frames.length} composites · {archiveMode?"full archive":"last 24 hours"}</span></div>{error&&<span role="alert">{error}</span>}</section>
  </main>
}
