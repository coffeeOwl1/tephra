use iced::Color;

// -- Base layers --
pub const OBSIDIAN: Color = Color::from_rgb(0.047, 0.035, 0.031); // #0C0908
pub const BASALT: Color = Color::from_rgb(0.086, 0.067, 0.063); // #161110
pub const SCORIA: Color = Color::from_rgb(0.137, 0.110, 0.094); // #231C18

// -- Text --
pub const PUMICE: Color = Color::from_rgb(0.910, 0.867, 0.831); // #E8DDD4
pub const TEPHRA: Color = Color::from_rgb(0.545, 0.494, 0.455); // #8B7E74

// -- Temperature scale --
pub const SANDSTONE: Color = Color::from_rgb(0.769, 0.584, 0.416); // #C4956A
pub const EMBER: Color = Color::from_rgb(0.878, 0.482, 0.235); // #E07B3C
pub const MAGMA: Color = Color::from_rgb(0.863, 0.247, 0.102); // #DC3F1A
pub const ERUPTION: Color = Color::from_rgb(1.000, 0.176, 0.102); // #FF2D1A

// -- Metric colors --
pub const COPPER: Color = Color::from_rgb(0.831, 0.569, 0.369); // #D4915E
pub const MINERAL: Color = Color::from_rgb(0.494, 0.722, 0.635); // #7EB8A2
pub const LAVA: Color = Color::from_rgb(0.910, 0.639, 0.235); // #E8A33C

// -- Semantic --
pub const GEOTHERMAL: Color = Color::from_rgb(0.420, 0.686, 0.482); // #6BAF7B

/// Get temperature color based on degrees Celsius.
/// Thresholds match reference: 70/80/90/95.
pub fn temp_color(temp_c: i32) -> Color {
    if temp_c >= 95 {
        ERUPTION
    } else if temp_c >= 90 {
        MAGMA
    } else if temp_c >= 80 {
        EMBER
    } else if temp_c >= 70 {
        SANDSTONE
    } else {
        MINERAL // cool/normal — uses the green tint
    }
}

/// Get utilization color (0-100%).
pub fn util_color(pct: f64) -> Color {
    if pct >= 85.0 {
        MAGMA
    } else if pct >= 50.0 {
        EMBER
    } else {
        LAVA
    }
}

/// Get power color based on watts.
pub fn power_color(watts: f64) -> Color {
    if watts >= 100.0 {
        MAGMA
    } else if watts >= 40.0 {
        EMBER
    } else {
        COPPER
    }
}

/// Create a color with modified alpha.
pub fn with_alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

/// Distinct colors for multi-node comparison charts.
const NODE_PALETTE: [Color; 8] = [
    EMBER,      // orange
    MINERAL,    // teal
    LAVA,       // amber
    GEOTHERMAL, // green
    COPPER,     // warm tan
    ERUPTION,   // red
    SANDSTONE,  // sandy
    PUMICE,     // light
];

/// Get a distinct color for a node by index.
pub fn node_color(index: usize) -> Color {
    NODE_PALETTE[index % NODE_PALETTE.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors_eq(a: Color, b: Color) -> bool {
        (a.r - b.r).abs() < f32::EPSILON
            && (a.g - b.g).abs() < f32::EPSILON
            && (a.b - b.b).abs() < f32::EPSILON
    }

    // T2-1: temp_color boundary tests
    #[test]
    fn temp_color_boundaries() {
        assert!(colors_eq(temp_color(50), MINERAL));  // cool
        assert!(colors_eq(temp_color(69), MINERAL));  // just below 70
        assert!(colors_eq(temp_color(70), SANDSTONE)); // boundary
        assert!(colors_eq(temp_color(79), SANDSTONE));
        assert!(colors_eq(temp_color(80), EMBER));     // boundary
        assert!(colors_eq(temp_color(89), EMBER));
        assert!(colors_eq(temp_color(90), MAGMA));     // boundary
        assert!(colors_eq(temp_color(94), MAGMA));
        assert!(colors_eq(temp_color(95), ERUPTION));  // boundary
        assert!(colors_eq(temp_color(105), ERUPTION)); // extreme
    }

    // T2-2: util_color boundary tests
    #[test]
    fn util_color_boundaries() {
        assert!(colors_eq(util_color(0.0), LAVA));
        assert!(colors_eq(util_color(49.9), LAVA));
        assert!(colors_eq(util_color(50.0), EMBER));   // boundary
        assert!(colors_eq(util_color(84.9), EMBER));
        assert!(colors_eq(util_color(85.0), MAGMA));   // boundary
        assert!(colors_eq(util_color(100.0), MAGMA));
    }

    // T2-3: power_color boundary tests
    #[test]
    fn power_color_boundaries() {
        assert!(colors_eq(power_color(0.0), COPPER));
        assert!(colors_eq(power_color(39.9), COPPER));
        assert!(colors_eq(power_color(40.0), EMBER));  // boundary
        assert!(colors_eq(power_color(99.9), EMBER));
        assert!(colors_eq(power_color(100.0), MAGMA)); // boundary
        assert!(colors_eq(power_color(200.0), MAGMA));
    }

    // T2-4: with_alpha preserves RGB
    #[test]
    fn with_alpha_preserves_rgb() {
        let original = EMBER;
        let modified = with_alpha(original, 0.5);
        assert!((modified.r - original.r).abs() < f32::EPSILON);
        assert!((modified.g - original.g).abs() < f32::EPSILON);
        assert!((modified.b - original.b).abs() < f32::EPSILON);
        assert!((modified.a - 0.5).abs() < f32::EPSILON);
        // Original alpha should be 1.0
        assert!((original.a - 1.0).abs() < f32::EPSILON);
    }
}
