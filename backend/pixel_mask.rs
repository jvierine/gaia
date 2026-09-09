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

/// Shortest distance from a point to a segment, both in the same units.
fn segment_distance(p:[f64;2],a:[f64;2],b:[f64;2])->f64{
    let (dx,dy)=(b[0]-a[0],b[1]-a[1]);
    let len2=dx*dx+dy*dy;
    let t=if len2<=0. {0.} else {(((p[0]-a[0])*dx+(p[1]-a[1])*dy)/len2).clamp(0.,1.)};
    ((p[0]-a[0]-t*dx).powi(2)+(p[1]-a[1]-t*dy).powi(2)).sqrt()
}

/// Distance from a normalized image point to the nearest edge of the kept
/// region: the crop rectangle, or any obstruction polygon. `scale` converts
/// normalized coordinates into the pixel grid the mesh was rasterized on, so the
/// result is a pixel distance. Points already outside clamp to zero.
pub fn boundary_distance_px(p:[f64;2],crop:[f64;4],polygons:&[Vec<[f64;2]>],scale:[f64;2])->f64{
    let (x,y)=(p[0]*scale[0],p[1]*scale[1]);
    let mut best=[
        x-crop[0]*scale[0],
        crop[2]*scale[0]-x,
        y-crop[1]*scale[1],
        crop[3]*scale[1]-y,
    ].into_iter().fold(f64::INFINITY,f64::min);
    for polygon in polygons {
        if polygon.len()<3 {continue}
        for i in 0..polygon.len(){
            let a=polygon[i];let b=polygon[(i+1)%polygon.len()];
            best=best.min(segment_distance(
                [x,y],
                [a[0]*scale[0],a[1]*scale[1]],
                [b[0]*scale[0],b[1]*scale[1]],
            ));
        }
    }
    best.max(0.)
}

#[cfg(test)] mod tests{
    use super::*;
    #[test] fn boundary_distance_measures_crop_and_obstructions(){
        let scale=[256.,256.];
        // No obstruction: the crop rectangle edge is the only boundary.
        let full=boundary_distance_px([0.5,0.5],[0.,0.,1.,1.],&[],scale);
        assert!((full-128.).abs()<1e-9,"centre of a full frame is half a frame from the edge");
        // One pixel inside the frame edge is one pixel from the boundary.
        assert!((boundary_distance_px([1./256.,0.5],[0.,0.,1.,1.],&[],scale)-1.).abs()<1e-9);
        // A tighter crop moves the boundary in.
        assert!((boundary_distance_px([0.5,0.5],[0.25,0.,1.,1.],&[],scale)-64.).abs()<1e-9);
        // Outside the kept region clamps to zero rather than going negative.
        assert_eq!(boundary_distance_px([0.1,0.5],[0.25,0.,1.,1.],&[],scale),0.);
        // An obstruction edge is nearer than the frame edge.
        let square=vec![vec![[0.5,0.4],[0.7,0.4],[0.7,0.6],[0.5,0.6]]];
        let d=boundary_distance_px([0.4,0.5],[0.,0.,1.,1.],&square,scale);
        assert!((d-0.1*256.).abs()<1e-9,"expected distance to the polygon edge, got {d}");
        // Anisotropic grids are measured in their own pixels.
        assert!((boundary_distance_px([0.5,0.5],[0.,0.,1.,1.],&[],[256.,192.])-96.).abs()<1e-9);
    }

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
