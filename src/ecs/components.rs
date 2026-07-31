//! Game components, carried on engine entities in the app member world.
//!
//! What lives here is what the demo creates and destroys one at a time: a grown
//! ice prism, and a spell that has been cast. Everything with a fixed population
//! written straight into a data texture each frame stays a resource, because a
//! pool of five thousand snow grains is not five thousand entities.

/// One grown ice prism.
///
/// Ninety-odd of these stand at once, each with its own age, lifetime and growth
/// rate, and each despawning on its own schedule. Growth is not a uniform scale:
/// height leads and girth follows, so a crystal spears up and then thickens.
#[derive(Default, Clone, Copy, Debug)]
pub struct Crystal {
    pub position: [f32; 3],
    pub axis: [f32; 3],
    pub height: f32,
    pub radius: f32,
    pub seed: f32,
    pub age: f32,
    /// Seconds at full size before it starts to sublimate.
    pub life: f32,
    /// Seconds it spends growing from nothing.
    pub grow: f32,
}

/// A crescent of slush running outward from where it was cast.
#[derive(Default, Clone, Copy, Debug)]
pub struct Sweep {
    pub strand: Option<usize>,
    pub time: f32,
    pub origin: [f32; 2],
    pub direction: [f32; 2],
    /// Metres the crest has travelled from the origin.
    pub reach: f32,
    pub brush_owed: f32,
    pub spray_owed: f32,
}

/// A held stream of water tracking the hand and the aim.
///
/// The spine is a record of where the tip has been rather than a shape
/// recomputed from the current aim, which is what gives the body its momentum.
#[derive(Clone, Copy, Debug)]
pub struct Ribbon {
    pub held: bool,
    pub strand: Option<usize>,
    pub position: [[f32; 3]; RIBBON_SAMPLES],
    /// Tip speed when each sample was laid, which is where the body's thickness
    /// variation comes from.
    pub speed: [f32; RIBBON_SAMPLES],
    pub head: usize,
    pub count: usize,
    pub tip: [f32; 3],
    pub velocity: [f32; 3],
    pub phase: f32,
    pub blend: f32,
    pub spray_owed: f32,
    pub score_owed: f32,
    pub retire_owed: f32,
    pub thrown: bool,
    pub splashed: bool,
    pub throw_time: f32,
    pub throw_aim: [f32; 3],
}

/// Live spine samples on a ribbon, capped by the strand table's width.
pub const RIBBON_SAMPLES: usize = 46;

impl Default for Ribbon {
    fn default() -> Self {
        Self {
            held: false,
            strand: None,
            position: [[0.0; 3]; RIBBON_SAMPLES],
            speed: [0.0; RIBBON_SAMPLES],
            head: 0,
            count: 0,
            tip: [0.0; 3],
            velocity: [0.0; 3],
            phase: 0.0,
            blend: 0.0,
            spray_owed: 0.0,
            score_owed: 0.0,
            retire_owed: 0.0,
            thrown: false,
            splashed: false,
            throw_time: 0.0,
            throw_aim: [0.0, 0.0, 1.0],
        }
    }
}

/// A targeted eruption: a column, a crater, and a long fallout.
#[derive(Default, Clone, Copy, Debug)]
pub struct Bloom {
    pub strand: Option<usize>,
    pub time: f32,
    pub centre: [f32; 3],
    pub lean: [f32; 2],
    pub burst: bool,
    pub curtain_owed: f32,
}

/// Water snapping to ice, planting a formation along a spiral.
#[derive(Default, Clone, Copy, Debug)]
pub struct Crystallize {
    pub time: f32,
    pub centre: [f32; 3],
    pub planted: usize,
    pub seed: f32,
}

/// A column of lifted snow that strips the ground and gives it back.
#[derive(Clone, Copy, Debug)]
pub struct Vortex {
    pub strands: [Option<usize>; VORTEX_HELICES],
    pub time: f32,
    pub centre: [f32; 2],
    pub spin: f32,
    pub strip_owed: f32,
    pub grain_owed: f32,
    /// How far out the stripping ring has reached, in metres.
    pub ring: f32,
}

/// How many helices a vortex winds. Three reads as a spiral; two reads as a
/// double helix.
pub const VORTEX_HELICES: usize = 3;

impl Default for Vortex {
    fn default() -> Self {
        Self {
            strands: [None; VORTEX_HELICES],
            time: 0.0,
            centre: [0.0; 2],
            spin: 0.0,
            strip_owed: 0.0,
            grain_owed: 0.0,
            ring: 0.9,
        }
    }
}
