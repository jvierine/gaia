//! Conservative image-quality signals. Production deployments may add a learned cloud model,
//! but every pixel keeps explicit cloud and obstruction probabilities.
pub fn robust_affine(reference: &[f32], candidate: &[f32], valid: &[bool]) -> (f32, f32) {
    let mut pairs: Vec<(f32, f32)> = reference
        .iter()
        .zip(candidate)
        .zip(valid)
        .filter_map(|((&r, &c), &v)| v.then_some((r, c)))
        .collect();
    if pairs.len() < 8 {
        return (1.0, 0.0);
    }
    pairs.sort_by(|a, b| a.1.total_cmp(&b.1));
    let lo = pairs.len() / 10;
    let hi = pairs.len() - lo;
    let p = &pairs[lo..hi];
    let (mr, mc) = p
        .iter()
        .fold((0.0, 0.0), |(ar, ac), (r, c)| (ar + r, ac + c));
    let n = p.len() as f32;
    let (mr, mc) = (mr / n, mc / n);
    let (cov, var) = p.iter().fold((0.0, 0.0), |(co, va), (r, c)| {
        (co + (c - mc) * (r - mr), va + (c - mc) * (c - mc))
    });
    let gain = if var > 1e-8 {
        (cov / var).clamp(0.2, 5.0)
    } else {
        1.0
    };
    (gain, mr - gain * mc)
}

pub fn cloud_probability(rgb: [u8; 3], local_texture: f32, star_support: f32) -> f32 {
    let [r, g, b] = rgb.map(|v| v as f32 / 255.0);
    let brightness = (r + g + b) / 3.0;
    let grayness = 1.0 - (r.max(g).max(b) - r.min(g).min(b));
    (0.50 * brightness + 0.30 * grayness + 0.35 * (1.0 - local_texture) - 0.45 * star_support)
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn affine_recovers_scale() {
        let c: Vec<f32> = (0..100).map(|x| x as f32).collect();
        let r: Vec<f32> = c.iter().map(|x| 2.0 * x + 3.0).collect();
        let (g, b) = robust_affine(&r, &c, &vec![true; 100]);
        assert!((g - 2.0).abs() < 1e-4);
        assert!((b - 3.0).abs() < 1e-3)
    }
}
