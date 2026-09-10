/** Inverse orthographic projection onto the actual 100-km image shell. */
export function shellTextureCoordinates(px:number,py:number,yaw:number,pitch:number):[number,number]|null {
 const radius=1+100/6371,r2=px*px+py*py;if(r2>radius*radius)return null;
 const z=Math.sqrt(radius*radius-r2),cy=Math.cos(pitch)*py-Math.sin(pitch)*z,cz=Math.sin(pitch)*py+Math.cos(pitch)*z;
 const wx=Math.cos(yaw)*px+Math.sin(yaw)*cz,wz=-Math.sin(yaw)*px+Math.cos(yaw)*cz;
 return [(Math.atan2(wx,wz)/(2*Math.PI)+1.5)%1,.5-Math.asin(Math.max(-1,Math.min(1,cy/radius)))/Math.PI];
}
