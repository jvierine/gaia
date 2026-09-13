import React from 'react';
import {LIVE_DELAY_MINUTES} from './live-time';
import './playback-toolbar.css';
export const PLAYBACK_SPEEDS=[0.25,0.5,1,2,4,8,16,32] as const;
type Props={playing:boolean;onPlay:()=>void;speed:number;onSpeed:(v:number)=>void;sunLock:boolean;onSunLock:(v:boolean)=>void;at:string|null;value:number;max:number;onScrub:(v:number)=>void;onPause:()=>void;onLatest:()=>void;disabled?:boolean;day?:string|null;children?:React.ReactNode};
export default function PlaybackToolbar(p:Props){return <section className="gaia-playback-dock" aria-label="Aurora playback">
<button className="dock-play" aria-label={p.playing?'Pause playback':'Play 24-hour overview'} disabled={p.disabled} onClick={p.onPlay}>{p.playing?'Ⅱ':'▶'}</button>
<div className="dock-speed" role="group" aria-label="Playback speed"><button aria-label="Slow the animation down" title="Slow down" disabled={p.speed<=PLAYBACK_SPEEDS[0]} onClick={()=>p.onSpeed(PLAYBACK_SPEEDS[Math.max(0,PLAYBACK_SPEEDS.indexOf(p.speed as typeof PLAYBACK_SPEEDS[number])-1)])}>−</button><span aria-live="polite">{p.speed}×</span><button aria-label="Speed the animation up" title="Speed up" disabled={p.speed>=32} onClick={()=>p.onSpeed(PLAYBACK_SPEEDS[Math.min(PLAYBACK_SPEEDS.length-1,PLAYBACK_SPEEDS.indexOf(p.speed as typeof PLAYBACK_SPEEDS[number])+1)])}>+</button></div>
<label className="dock-sun" title="Hold the sun upwards; drag vertically to change the viewing tilt"><input type="checkbox" checked={p.sunLock} onChange={e=>p.onSunLock(e.target.checked)}/><strong>Sun up</strong></label>
<div className="dock-track"><div className="dock-label"><strong>{p.day||'LAST 24 HOURS'}</strong><time>{p.at?new Date(p.at).toISOString().replace('T',' ').replace('.000Z',' UTC'):'Waiting for publication'}</time></div>
<input aria-label="Observation time" disabled={p.disabled} type="range" min={0} max={p.max} value={p.value} onPointerDown={p.onPause} onChange={e=>{p.onPause();p.onScrub(Number(e.target.value))}}/>
<div className="dock-ticks">{(p.day?['00:00','06:00','12:00','18:00']:['−24 h','−18 h','−12 h','−6 h']).map(t=><span key={t}>{t}</span>)}<button disabled={p.disabled} onClick={p.onLatest} title="Return to the latest ready images">{p.day?'Latest':'now −'+LIVE_DELAY_MINUTES+' min'}</button></div></div>{p.children}</section>}
