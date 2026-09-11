// Same Meeus/C-infinity equations as backend/geometry.rs. Geometry-independent
// IGRF, horizon and mask factors are supplied in the prepared vertex weights.
export type Rules={sun_dark_deg:number;sun_light_deg:number;sun_floor:number};
export function smoothStep(t:number){const psi=(x:number)=>x>0?Math.exp(-1/Math.abs(x)):0;const rise=psi(t);return rise===0?0:rise/(rise+psi(1-t))}
export function solarElevation(lat:number,lon:number,epoch:number){
  const rad=Math.PI/180,jd=epoch/86400000+2440587.5,t=(jd-2451545)/36525;
  const l0=(280.46646+t*(36000.76983+t*.0003032))*rad,m=(357.52911+t*(35999.05029-.0001537*t))*rad;
  const lambda=l0+((1.914602-.004817*t-.000014*t*t)*Math.sin(m)+.019993*Math.sin(2*m)+.000289*Math.sin(3*m))*rad;
  const epsilon=(23.439291-.0130042*t)*rad,decl=Math.asin(Math.sin(epsilon)*Math.sin(lambda)),ra=Math.atan2(Math.cos(epsilon)*Math.sin(lambda),Math.cos(lambda));
  const gmst=(280.46061837+360.98564736629*(jd-2451545)+.000387933*t*t-t*t*t/38710000)*rad;
  const dot=Math.sin(lat*rad)*Math.sin(decl)+Math.cos(lat*rad)*Math.cos(decl)*Math.cos(lon*rad-(ra-gmst));
  return Math.asin(Math.max(-1,Math.min(1,dot)))/rad;
}
export function cameraWeightScale(camera:{latitude_deg:number;longitude_deg:number;quality_exponent?:number},epoch:number,r:Rules){
  const sun=solarElevation(camera.latitude_deg,camera.longitude_deg,epoch),t=(sun-r.sun_dark_deg)/(r.sun_light_deg-r.sun_dark_deg);
  return Math.pow(2,Math.max(-8,Math.min(0,camera.quality_exponent||0)))*(r.sun_floor+(1-r.sun_floor)*(1-smoothStep(t)));
}
