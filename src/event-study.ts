export type EventMedia={
  id:string;source:string;kind:'image'|'timelapse';title:string|null;creator:string|null;
  location:string;latitude:number;longitude:number;capturedAt:string;capturedUntil:string|null;
  timePrecision:string;timeSource:string;coordinateSource:string;coordinatePrecision:string;
  grade:string;thumbnailUrl:string;previewUrl:string;mediaUrl:string|null;sourceUrl:string;
  originalUrl:string|null;license:string|null;rightsNote:string|null;
};

export type EventManifest={
  schema:'gaia-event-study-v1';eventId:string;title:string;description:string;generatedAt:string;
  binMinutes:number;items:EventMedia[];excluded:{untimed:number;unlocated:number;unavailable:number};
};

export type TimeBin={at:number;until:number;items:EventMedia[]};

const candidateMinutes=[1,2,5,10,15,30,60];

export function chooseBinMinutes(items:Pick<EventMedia,'capturedAt'>[],targetP90=24):number{
  const times=items.map(item=>Date.parse(item.capturedAt)).filter(Number.isFinite);
  if(times.length<2)return 5;
  for(const minutes of candidateMinutes){
    const counts=new Map<number,number>(),width=minutes*60_000;
    for(const at of times){const key=Math.floor(at/width);counts.set(key,(counts.get(key)||0)+1)}
    const ordered=[...counts.values()].sort((a,b)=>a-b);
    const p90=ordered[Math.min(ordered.length-1,Math.floor(ordered.length*.9))]||0;
    if(p90>=Math.min(16,targetP90*2/3)&&p90<=targetP90)return minutes;
  }
  return 60;
}

export function buildTimeBins(items:EventMedia[],minutes:number):TimeBin[]{
  const width=Math.max(1,minutes)*60_000,bins=new Map<number,EventMedia[]>();
  for(const item of items){
    const at=Date.parse(item.capturedAt);if(!Number.isFinite(at))continue;
    const key=Math.floor(at/width)*width;
    const list=bins.get(key)||[];list.push(item);bins.set(key,list);
  }
  const result=[...bins].sort((a,b)=>a[0]-b[0]).map(([at,binItems])=>({at,until:at+width,items:binItems.sort((a,b)=>Date.parse(a.capturedAt)-Date.parse(b.capturedAt))}));
  // A timelapse belongs to every already-populated bin it covers. We do not
  // invent empty time steps just because a video's published range is broad.
  const videos=items.filter(item=>item.kind==='timelapse'&&item.capturedUntil);
  for(const bin of result)for(const video of videos){
    const from=Date.parse(video.capturedAt),until=Date.parse(video.capturedUntil!);
    if(from<bin.until&&until>=bin.at&&!bin.items.some(item=>item.id===video.id))bin.items.unshift(video);
  }
  return result;
}

export type GlobeViewState={yaw:number;pitch:number;zoom:number;width:number;height:number};
export function projectEventLocation(latitude:number,longitude:number,view:GlobeViewState){
  const lat=latitude*Math.PI/180,lon=longitude*Math.PI/180;
  const x=Math.cos(lat)*Math.sin(lon),y=Math.sin(lat),z=Math.cos(lat)*Math.cos(lon);
  const xx=Math.cos(view.yaw)*x-Math.sin(view.yaw)*z;
  const zz=Math.sin(view.yaw)*x+Math.cos(view.yaw)*z;
  const yy=Math.cos(view.pitch)*y+Math.sin(view.pitch)*zz;
  const depth=-Math.sin(view.pitch)*y+Math.cos(view.pitch)*zz;
  const side=Math.min(view.width,view.height);
  return {x:view.width/2+xx*view.zoom*side/2,y:view.height/2-yy*view.zoom*side/2,visible:depth>=0,depth};
}

export type EventCluster={x:number;y:number;items:EventMedia[]};
export function clusterVisibleMedia(items:EventMedia[],view:GlobeViewState,cellPixels=94):EventCluster[]{
  const cells=new Map<string,EventCluster>();
  for(const item of items){
    const point=projectEventLocation(item.latitude,item.longitude,view);if(!point.visible)continue;
    const key=`${Math.round(point.x/cellPixels)}:${Math.round(point.y/cellPixels)}`;
    const cluster=cells.get(key);
    if(cluster){const n=cluster.items.length;cluster.x=(cluster.x*n+point.x)/(n+1);cluster.y=(cluster.y*n+point.y)/(n+1);cluster.items.push(item)}
    else cells.set(key,{x:point.x,y:point.y,items:[item]});
  }
  return [...cells.values()].sort((a,b)=>b.items.length-a.items.length);
}
