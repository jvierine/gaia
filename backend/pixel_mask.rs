//! Obstruction polygons in the uncropped, normalized image coordinate system.
pub fn inside(p:[f64;2], polygon:&[[f64;2]])->bool {
    let mut inside=false;
    for i in 0..polygon.len(){
        let a=polygon[i];let b=polygon[(i+1)%polygon.len()];
        if (a[1]>p[1])!=(b[1]>p[1]) && p[0]<(b[0]-a[0])*(p[1]-a[1])/(b[1]-a[1])+a[0]{inside=!inside;}
    }
    inside
}

// Conservative pixel coverage: reject even partial overlap with an obstruction.
pub fn blocked(rect:[f64;4],polygon:&[[f64;2]])->bool {
    if polygon.len()<3{return false}
    let [l,t,r,b]=rect;
    if [[l,t],[r,t],[r,b],[l,b]].iter().any(|p|inside(*p,polygon)){return true}
    for i in 0..polygon.len(){
        let a=polygon[i];let z=polygon[(i+1)%polygon.len()];
        // Clip each polygon edge against the pixel rectangle.
        let(mut lo,mut hi)=(0.0_f64,1.0_f64);
        for (axis,min,max) in [(0,l,r),(1,t,b)]{
            let d=z[axis]-a[axis];
            if d.abs()<1e-15 {if a[axis]<min||a[axis]>max{hi=-1.;break}}
            else{let u=(min-a[axis])/d;let v=(max-a[axis])/d;lo=lo.max(u.min(v));hi=hi.min(u.max(v));}
        }
        if lo<=hi{return true}
    }
    false
}

pub fn allowed(rect:[f64;4],crop:[f64;4],polygons:&[Vec<[f64;2]>])->bool{
    rect[0]>=crop[0]&&rect[1]>=crop[1]&&rect[2]<=crop[2]&&rect[3]<=crop[3]
        && !polygons.iter().any(|p|blocked(rect,p))
}

#[cfg(test)] mod tests{
    use super::*;
    #[test] fn multiple_obstructions_and_crop(){
        let masks=vec![vec![[0.,0.],[0.2,0.],[0.2,0.2],[0.,0.2]],vec![[0.8,0.8],[1.2,0.8],[1.2,1.2],[0.8,1.2]]];
        assert!(!allowed([0.1,0.1,0.12,0.12],[0.,0.,1.,1.],&masks));
        assert!(!allowed([0.9,0.9,0.92,0.92],[0.,0.,1.,1.],&masks));
        assert!(allowed([0.4,0.4,0.42,0.42],[0.,0.,1.,1.],&masks));
        assert!(!allowed([0.4,0.4,0.42,0.42],[0.,0.41,1.,1.],&masks));
    }
    #[test] fn small_and_crossing_polygons(){
        assert!(blocked([0.,0.,1.,1.],&[[0.4,0.4],[0.6,0.4],[0.5,0.6]]));
        assert!(blocked([0.,0.,1.,1.],&[[-1.,0.4],[2.,0.4],[2.,0.6],[-1.,0.6]]));
        assert!(!blocked([0.,0.,1.,1.],&[[2.,2.],[3.,2.],[3.,3.]]));
    }
}
