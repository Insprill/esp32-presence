use std::time::SystemTime;

pub fn map_range(x: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> u8 {
    let x = x.clamp(in_min, in_max);
    let mapped = (x - in_min) * (out_max - out_min) / (in_max - in_min) + out_min;
    mapped.clamp(out_min, out_max) as u8
}

pub fn unix_seconds() -> u32 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap() // Not possible to panic
        .as_secs() as u32
}
