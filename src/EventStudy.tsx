import React,{useEffect,useMemo,useRef,useState} from 'react';
import {ExternalLink,Film,Pause,Play,SlidersHorizontal,SquareDashedMousePointer,X} from 'lucide-react';
import GaiaLogo from './GaiaLogo';
import GaiaGlobeView from './GaiaGlobeView';
import MaskEditor from './MaskEditor';
import {buildTimeBins,clusterVisibleMedia,type EventCluster,type EventManifest,type EventMedia,type GlobeViewState} from './event-study';
import './event-study.css';

const utc=(value:string|number,seconds=true)=>{const iso=new Date(value).toISOString();return seconds?`${iso.slice(0,19)} UTC`:`${iso.slice(0,16)} UTC`};
const coordinate=(value:number,positive:string,negative:string)=>`${Math.abs(value).toFixed(3)}°${value>=0?positive:negative}`;
const eventIdFromPath=()=>location.pathname.match(/\/events\/(\d{8})(?:\/|$)/)?.[1]||'';

function MediaDetail({item,eventId,calibrated,onMask,onClose}:{item:EventMedia;eventId:string;calibrated:boolean;onMask:()=>void;onClose:()=>void}){
  const returnUrl=location.href;
  const aida=new URL('/aida/',location.origin);
  for(const [key,value] of Object.entries({gaia_event:'1',event_id:eventId,record_id:item.id,image_url:item.previewUrl,latitude_deg:String(item.latitude),longitude_deg:String(item.longitude),observation_utc:item.capturedAt,return_url:returnUrl}))aida.searchParams.set(key,value);
  return <aside className="event-detail" aria-label="Selected event image">
    <button className="event-detail-close" onClick={onClose} aria-label="Close image details"><X size={18}/></button>
    <div className="event-detail-image">{item.mediaUrl?<video src={item.mediaUrl} poster={item.previewUrl} controls playsInline preload="metadata" aria-label={item.title||`Aurora timelapse by ${item.creator||'unknown photographer'}`}/>:<img src={item.previewUrl} alt={item.title||`Aurora by ${item.creator||'unknown photographer'}`}/>} {item.kind==='timelapse'&&<span><Film size={14}/> timelapse</span>}</div>
    <div className="event-detail-body">
      <span className="event-kicker">{item.source.replaceAll('-',' ')}</span>
      <h2>{item.title||item.creator||'Aurora observation'}</h2>
      {item.creator&&item.title&&<p>By {item.creator}</p>}
      <dl><div><dt>UTC</dt><dd>{utc(item.capturedAt)}</dd></div>{item.capturedUntil&&<div><dt>Ends</dt><dd>{utc(item.capturedUntil)}</dd></div>}<div><dt>Location</dt><dd>{item.location}<br/>{coordinate(item.latitude,'N','S')}, {coordinate(item.longitude,'E','W')}</dd></div><div><dt>Time basis</dt><dd>{item.timeSource} · {item.timePrecision}</dd></div><div><dt>Position basis</dt><dd>{item.coordinateSource} · {item.coordinatePrecision}</dd></div></dl>
      <div className="event-calibration-state" data-calibrated={calibrated}>{calibrated?'Calibrated · projected at 100 km':'Not calibrated · shown as a location marker'}</div>
      <div className="event-detail-actions"><a className="event-primary" href={aida.href} target="_blank" rel="noreferrer">{calibrated?'Recalibrate in AIDA':'Calibrate in AIDA'} <ExternalLink size={14}/></a><button type="button" onClick={onMask}><SquareDashedMousePointer size={14}/> Edit crop &amp; mask</button><a href={item.sourceUrl} target="_blank" rel="noreferrer">Open source posting <ExternalLink size={13}/></a></div>
      <p className="event-rights">{item.license||item.rightsNote||'Copyright remains with the photographer. Research preview only.'}</p>
    </div>
  </aside>;
}

function GlobeMedia({clusters,calibrated,onChoose}:{clusters:EventCluster[];calibrated:Set<string>;onChoose:(item:EventMedia)=>void}){
  return <div className="event-globe-media" aria-label="Images in the selected time interval">{clusters.map(cluster=>{
    const representative=cluster.items.find(item=>item.kind==='timelapse')||cluster.items[0];
    return <button key={cluster.items.map(item=>item.id).join('|')} className={`event-photo-marker${cluster.items.some(item=>calibrated.has(item.id))?' calibrated':''}`} style={{left:cluster.x,top:cluster.y}} onClick={()=>onChoose(representative)} title={`${cluster.items.length} image${cluster.items.length===1?'':'s'} near ${representative.location}`}>
      <img src={representative.thumbnailUrl} alt=""/>{cluster.items.length>1&&<strong>{cluster.items.length}</strong>}{representative.kind==='timelapse'&&<span><Film size={12}/></span>}
    </button>;
  })}</div>;
}

export default function EventStudy(){
  const eventId=eventIdFromPath();
  const [manifest,setManifest]=useState<EventManifest|null>(null),[error,setError]=useState('');
  const [index,setIndex]=useState(0),[playing,setPlaying]=useState(false),[speed,setSpeed]=useState(1);
  const [view,setView]=useState<GlobeViewState|null>(null),[selected,setSelected]=useState<EventMedia|null>(null),[masking,setMasking]=useState<EventMedia|null>(null),[source,setSource]=useState('all');
  const [projectionRevision,setProjectionRevision]=useState(0),[calibrated,setCalibrated]=useState<Set<string>>(new Set());
  const epoch=useRef(Date.parse('2025-11-12T03:00:00Z'));
  useEffect(()=>{const controller=new AbortController();fetch(`/gaia/public/events/${encodeURIComponent(eventId)}/manifest.json`,{cache:'no-store',signal:controller.signal}).then(async response=>{if(!response.ok)throw Error(`Event dataset unavailable (${response.status})`);return response.json()}).then((value:EventManifest)=>{setManifest(value);setError('')}).catch(reason=>{if(!controller.signal.aborted)setError(reason.message||String(reason))});return()=>controller.abort()},[eventId]);
  useEffect(()=>{const controller=new AbortController();const refresh=()=>fetch(`/gaia/api/events/${encodeURIComponent(eventId)}/projection-manifest`,{cache:'no-store',signal:controller.signal}).then(r=>r.ok?r.json():Promise.reject(Error(`Projection catalogue unavailable (${r.status})`))).then(value=>setCalibrated(new Set((value.cameras||[]).map((camera:{source_id:string})=>camera.source_id)))).catch(reason=>{if(!controller.signal.aborted)console.warn(reason)});refresh();const timer=setInterval(refresh,15000);window.addEventListener('focus',refresh);return()=>{controller.abort();clearInterval(timer);window.removeEventListener('focus',refresh)}},[eventId,projectionRevision]);
  useEffect(()=>{const calibrated=(message:MessageEvent)=>{if(message.origin===location.origin&&message.data?.type==='gaia-event-calibrated'&&message.data.eventId===eventId)setProjectionRevision(value=>value+1)};window.addEventListener('message',calibrated);return()=>window.removeEventListener('message',calibrated)},[eventId]);
  const allBins=useMemo(()=>manifest?buildTimeBins(manifest.items,manifest.binMinutes):[],[manifest]);
  const bins=useMemo(()=>source==='all'?allBins:allBins.map(bin=>({...bin,items:bin.items.filter(item=>item.source===source)})).filter(bin=>bin.items.length),[allBins,source]);
  useEffect(()=>{setIndex(0);setPlaying(false)},[source]);
  useEffect(()=>{if(!playing||bins.length<2)return;const timer=setInterval(()=>setIndex(value=>(value+1)%bins.length),Math.max(120,1100/speed));return()=>clearInterval(timer)},[playing,speed,bins.length]);
  useEffect(()=>{if(bins[index])epoch.current=bins[index].until-1},[bins,index]);
  const bin=bins[Math.min(index,Math.max(0,bins.length-1))];
  const clusters=useMemo(()=>bin&&view?clusterVisibleMedia(bin.items,view):[],[bin,view]);
  const sources=useMemo(()=>manifest?[...new Set(manifest.items.map(item=>item.source))].sort():[],[manifest]);
  return <main className="event-study">
    <header className="event-header"><a className="event-brand" href="/gaia/"><GaiaLogo/><span><strong>GAIA</strong><small>EVENT STUDY</small></span></a><div><span className="event-kicker">G4 geomagnetic storm</span><h1>{manifest?.title||'11–12 November 2025'}</h1></div><div className="event-count"><strong>{manifest?.items.length.toLocaleString()||'—'}</strong><span>located observations</span></div></header>
    <section className="event-stage">
      <GaiaGlobeView className="event-globe" getEpochMillis={()=>epoch.current} mode="event" manifestUrl={`/gaia/api/events/${encodeURIComponent(eventId)}/projection-manifest?v=${projectionRevision}`} onViewState={setView}>
        {bin&&<GlobeMedia clusters={clusters} calibrated={calibrated} onChoose={setSelected}/>}
      </GaiaGlobeView>
      <div className="event-status"><span>{bin?`${bin.items.length} observations · ${clusters.length} visible groups · ${bin.items.filter(item=>calibrated.has(item.id)).length} projected`:'Loading event…'}</span><span>Uncalibrated photos stay pinned; calibrated photos are projected to the 100 km emission shell</span></div>
      {error&&<div className="event-error" role="alert">{error}</div>}
      {selected&&<MediaDetail item={selected} eventId={eventId} calibrated={calibrated.has(selected.id)} onMask={()=>setMasking(selected)} onClose={()=>setSelected(null)}/>}
      {masking&&<MaskEditor title={masking.title||masking.creator||masking.location} imageUrl={`/gaia/api/events/${encodeURIComponent(eventId)}/records/${encodeURIComponent(masking.id)}/image`} settingsUrl={`/gaia/api/events/${encodeURIComponent(eventId)}/records/${encodeURIComponent(masking.id)}/settings`} onSaved={()=>setProjectionRevision(value=>value+1)} onClose={()=>setMasking(null)}/>}
    </section>
    <section className="event-console" aria-label="Event playback controls">
      <div className="event-now"><span className="event-kicker">UTC interval</span><time>{bin?utc(bin.at,false):'—'}</time><small>{bin?`${manifest?.binMinutes} minute bin · exact capture times retained`:'Preparing timeline'}</small></div>
      <div className="event-transport"><button onClick={()=>setPlaying(value=>!value)} disabled={bins.length<2}>{playing?<><Pause size={15}/> Pause</>:<><Play size={15}/> Play storm</>}</button><input aria-label="Event time interval" type="range" min="0" max={Math.max(0,bins.length-1)} value={Math.min(index,Math.max(0,bins.length-1))} onChange={event=>{setPlaying(false);setIndex(Number(event.target.value))}}/><span>{bins.length?`${Math.min(index+1,bins.length)} / ${bins.length}`:'0 / 0'}</span></div>
      <div className="event-filters"><label><SlidersHorizontal size={14}/> Source<select value={source} onChange={event=>setSource(event.target.value)}><option value="all">All sources</option>{sources.map(value=><option value={value} key={value}>{value} · {manifest?.items.filter(item=>item.source===value).length}</option>)}</select></label><label>Speed<select value={speed} onChange={event=>setSpeed(Number(event.target.value))}>{[.5,1,2,4].map(value=><option key={value} value={value}>{value}×</option>)}</select></label></div>
      <div className="event-strip">{bin?.items.map(item=><button key={item.id} className={selected?.id===item.id?'selected':''} onClick={()=>setSelected(item)}><img src={item.thumbnailUrl} alt=""/><span>{new Date(item.capturedAt).toISOString().slice(11,19)}<small>{item.location}</small></span>{item.kind==='timelapse'&&<Film size={12}/>}</button>)}</div>
    </section>
  </main>;
}
