#define_import_path snow::terrain

#import snow::noise::{fbmDamped, ridgedd, noised, noise2, hash22}

// The landform.

// Build the combined rotate-and-anisotropically-scale matrix for a noise layer.
fn windMat(angle: f32, sx: f32, sy: f32, scale: f32) -> mat2x2f {
    let c = cos(angle);
    let s = sin(angle);
    let r = mat2x2f(c, -s, s, c);
    let d = mat2x2f(sx / scale, 0.0, 0.0, sy / scale);
    return d * r;
}

// ------------------------------------------------------------------- macro

// Broad and medium landform.
fn terrainMacro(p: vec2f, w: f32, amp: f32) -> f32 {
    // --- broad dunes -------------------------------------------------------
    // Compressed along the wind, so ridge lines run across it.
    let m1 = windMat(w, 2.1, 1.0, 58.0);
    let broad = fbmDamped(m1 * p, 5, 2.03, 0.5, 0.9);
    var h = broad.x * 15.5;

    // A second, much larger and gentler swell so the field never reads as one repeating
    // dune wavelength.
    let m0 = windMat(w, 1.35, 1.0, 210.0);
    let swell = fbmDamped(m0 * p, 3, 2.11, 0.55, 0.3);
    h += swell.x * 26.0;

    // --- medium drifts and wind lobes -------------------------------------- The
    // domain is sheared along the wind by the broad height, which steepens lee faces
    // and flattens windward ones: dune asymmetry, near enough.
    let m2 = windMat(w, 1.55, 1.0, 13.5);
    var q2 = m2 * p;
    q2.x += broad.x * 2.4;
    let med = fbmDamped(q2, 4, 2.07, 0.48, 1.7);

    // Drifts pile up where the broad form is concave (troughs and lee pockets) and get
    // scoured off exposed crests.
    let shelter = clamp(0.5 - broad.x * 0.75, 0.15, 1.0);
    h += med.x * 2.9 * shelter;

    return h * amp;
}

// -------------------------------------------------------------------- rocks

// Sparse exposed rock.
fn rockField(p: vec2f, w: f32) -> vec2f {
    let cell = 165.0;
    let gi = floor(p / cell);

    var hSum = 0.0;
    var mask = 0.0;

    // 3x3 neighbourhood so blobs straddle cell borders cleanly.
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let id = gi + vec2f(f32(dx), f32(dy));
            let r = hash22(id);
            let r2 = hash22(id + 71.3);

            // Cull most cells: outcrops are meant to be sparse.
            if (r2.x > 0.34) { continue; }

            let centre = (id + 0.15 + r * 0.7) * cell;
            let radius = 7.0 + r2.y * 11.0;
            let d = length(p - centre);
            if (d > radius * 1.6) { continue; }

            // Smooth dome, then broken up by ridged noise so it reads as rock rather
            // than as a lump.
            let t = clamp(1.0 - d / radius, 0.0, 1.0);
            let dome = t * t * (3.0 - 2.0 * t);
            let mr = windMat(w, 1.0, 1.0, 5.5);
            let rough = ridgedd(mr * (p - centre), 3, 2.17, 0.55).x;
            let hgt = (3.5 + r2.y * 6.0);

            hSum += dome * hgt * (0.62 + 0.55 * rough);
            mask = max(mask, dome * dome);
        }
    }
    return vec2f(hSum, mask);
}

// --------------------------------------------------------------------- fine

// Local departure of the wind from its prevailing bearing, in radians, and the /
// local anisotropy of the sastrugi.
fn windLocal(p: vec2f) -> vec2f {
    let veer = noise2(p * 0.0083 + vec2f(31.7, 12.3)) * 0.42;
    let stretch = 2.3 + 2.4 * (noise2(p * 0.0126 + vec2f(7.1, 41.9)) * 0.5 + 0.5);
    return vec2f(veer, stretch);
}

// Sastrugi and ripples.
fn terrainFine(p: vec2f, w: f32, exposure: f32, amp: f32) -> vec3f {
    var h = 0.0;
    var d = vec2f(0.0);

    let wl = windLocal(p);

    // --- sastrugi ----------------------------------------------------------
    // Compressed across the wind, so the ridges streak along it.
    let m3 = windMat(w + wl.x, 1.0, wl.y, 2.3);
    let sas = ridgedd(m3 * p, 3, 2.11, 0.52);
    let scour = 0.45 + 0.55 * smoothstep(-0.25, 0.35, noise2(p * 0.021));
    let sasAmp = 0.125 * amp * mix(0.45, 1.0, exposure) * scour;
    h += (sas.x - 0.35) * sasAmp;
    d += (sas.yz * m3) * sasAmp;

    // --- wind ripples ------------------------------------------------------ Fine
    // transverse corrugation, strongest in the sheltered flats where sastrugi is weak.
    let m4 = windMat(w + wl.x * 0.5, 2.9, 1.0, 0.42);
    let rip = noised(m4 * p);
    let ripAmp = 0.024 * amp * mix(1.0, 0.45, exposure);
    h += rip.x * ripAmp;
    d += (rip.yz * m4) * ripAmp;

    // --- grain ------------------------------------------------------------- Sub-
    // centimetre.
    let m5 = windMat(w, 1.0, 1.0, 0.115);
    let gr = noised(m5 * p);
    let grAmp = 0.0075 * amp;
    h += gr.x * grAmp;
    d += (gr.yz * m5) * grAmp;

    return vec3f(h, d);
}

// Footprint-filtered fine layer, for the fragment shader.
fn terrainFineFiltered(p: vec2f, w: f32, exposure: f32, amp: f32, fp: f32) -> vec3f {
    var h = 0.0;
    var d = vec2f(0.0);

    // Sastrugi: wavelength around 2.3 m.
    let wl = windLocal(p);

    let fadeS = 1.0 - smoothstep(0.35, 1.6, fp);
    if (fadeS > 0.001) {
        let m3 = windMat(w + wl.x, 1.0, wl.y, 2.3);
        let sas = ridgedd(m3 * p, 3, 2.11, 0.52);
        // Modulated by a slow field so the surface has scoured patches and smooth
        // patches rather than one uniform corduroy everywhere.
        let scour = 0.45 + 0.55 * smoothstep(-0.25, 0.35, noise2(p * 0.021));
        let a = 0.125 * amp * mix(0.45, 1.0, exposure) * scour * fadeS;
        h += (sas.x - 0.35) * a;
        d += (sas.yz * m3) * a;
    }

    // Ripples: wavelength around 0.42 m.
    let fadeR = 1.0 - smoothstep(0.06, 0.3, fp);
    if (fadeR > 0.001) {
        let m4 = windMat(w + wl.x * 0.5, 2.9, 1.0, 0.42);
        let rip = noised(m4 * p);
        let a = 0.024 * amp * mix(1.0, 0.45, exposure) * fadeR;
        h += rip.x * a;
        d += (rip.yz * m4) * a;
    }

    // Grain: wavelength around 0.115 m.
    let fadeG = 1.0 - smoothstep(0.016, 0.08, fp);
    if (fadeG > 0.001) {
        let m5 = windMat(w, 1.0, 1.0, 0.115);
        let gr = noised(m5 * p);
        let a = 0.0075 * amp * fadeG;
        h += gr.x * a;
        d += (gr.yz * m5) * a;
    }

    return vec3f(h, d);
}
