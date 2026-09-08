// Add volcanic relief to the existing crust, not to an absolute zero-height
// cone. Both uplift and its slope vanish at the footprint edge, even when
// that edge lies over a deep seabed. Existing terrain detail is preserved.
fn hotspot_elevation(height: f32, distance: f32, radius: f32, peak: f32) -> f32 {
    let core = clamp(1.0 - distance / max(radius, 1.0e-6), 0.0, 1.0);
    return height + peak * core * core;
}

// Preserve broad continental contrast without an infinite slope at noise=0.
// sign(x)*abs(x)^0.35 turned smooth zero contours into cliff-like outlines.
// The regularized curve is odd, monotone, and retains +/-1 endpoints.
fn continental_elevation(value: f32) -> f32 {
    return value * pow((value * value + 0.04) / 1.04, -0.325);
}
