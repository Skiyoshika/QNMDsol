use std::f32::consts::PI;
#[derive(Clone, Copy, Debug)]
pub enum FilterKind {
    Notch { freq_hz: f32, q: f32 },
    Highpass { cutoff_hz: f32, q: f32 },
    Lowpass { cutoff_hz: f32, q: f32 },
    Bandpass { low_hz: f32, high_hz: f32, q: f32 },
    Bandstop { low_hz: f32, high_hz: f32, q: f32 },
}
#[derive(Clone, Copy, Debug)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}
#[derive(Clone, Copy, Debug, Default)]
struct BiquadState {
    z1: f32,
    z2: f32,
}
#[derive(Clone, Copy, Debug)]
struct BiquadFilter {
    coeffs: BiquadCoeffs,
    state: BiquadState,
}
impl BiquadFilter {
    fn new(coeffs: BiquadCoeffs) -> Self {
        Self {
            coeffs,
            state: BiquadState::default(),
        }
    }
    fn process(&mut self, input: f32) -> f32 {
        // Transposed direct form II
        let y = self.coeffs.b0 * input + self.state.z1;
        self.state.z1 = self.coeffs.b1 * input - self.coeffs.a1 * y + self.state.z2;
        self.state.z2 = self.coeffs.b2 * input - self.coeffs.a2 * y;
        y
    }
}
#[derive(Default, Debug)]
pub struct FilterChain {
    sections: Vec<BiquadFilter>,
}
impl FilterChain {
    pub fn empty() -> Self {
        Self { sections: vec![] }
    }
    pub fn from_kinds(sample_rate_hz: f32, kinds: &[FilterKind]) -> Self {
        let mut sections = Vec::new();
        for kind in kinds {
            sections.extend(design_sections(sample_rate_hz, *kind));
        }
        Self { sections }
    }
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
    pub fn process_sample(&mut self, mut value: f32) -> f32 {
        for section in &mut self.sections {
            value = section.process(value);
        }
        value
    }
}
fn design_sections(sample_rate_hz: f32, kind: FilterKind) -> Vec<BiquadFilter> {
    let nyquist = sample_rate_hz * 0.5;
    match kind {
        FilterKind::Notch { freq_hz, q } => {
            let coeffs = notch(nyquist_clamp(freq_hz, nyquist), sample_rate_hz, q);
            vec![BiquadFilter::new(coeffs)]
        }
        FilterKind::Highpass { cutoff_hz, q } => {
            let coeffs = highpass(nyquist_clamp(cutoff_hz, nyquist), sample_rate_hz, q);
            vec![BiquadFilter::new(coeffs)]
        }
        FilterKind::Lowpass { cutoff_hz, q } => {
            let coeffs = lowpass(nyquist_clamp(cutoff_hz, nyquist), sample_rate_hz, q);
            vec![BiquadFilter::new(coeffs)]
        }
        FilterKind::Bandpass { low_hz, high_hz, q } => {
            let (low, high) = band_edges(low_hz, high_hz, nyquist);
            let center = (low * high).sqrt();
            let q_val = q.max(0.1).min(100.0).min(center / (high - low));
            let coeffs = bandpass(center, sample_rate_hz, q_val);
            vec![BiquadFilter::new(coeffs)]
        }
        FilterKind::Bandstop { low_hz, high_hz, q } => {
            let (low, high) = band_edges(low_hz, high_hz, nyquist);
            let center = (low * high).sqrt();
            let q_val = q.max(0.1).min(100.0).min(center / (high - low));
            let coeffs = notch(center, sample_rate_hz, q_val);
            vec![BiquadFilter::new(coeffs)]
        }
    }
}
fn nyquist_clamp(freq_hz: f32, nyquist: f32) -> f32 {
    freq_hz.clamp(0.01, nyquist - 0.01)
}
fn band_edges(low_hz: f32, high_hz: f32, nyquist: f32) -> (f32, f32) {
    let low = nyquist_clamp(low_hz.min(high_hz), nyquist);
    let high = nyquist_clamp(low_hz.max(high_hz), nyquist);
    (low, high)
}
fn lowpass(freq_hz: f32, sample_rate_hz: f32, q: f32) -> BiquadCoeffs {
    let w0 = 2.0 * PI * freq_hz / sample_rate_hz;
    let alpha = (w0 / 2.0).sin() / (2.0 * q);
    let cos_w0 = w0.cos();
    let b0 = (1.0 - cos_w0) * 0.5;
    let b1 = 1.0 - cos_w0;
    let b2 = b0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;
    normalize(b0, b1, b2, a0, a1, a2)
}
fn highpass(freq_hz: f32, sample_rate_hz: f32, q: f32) -> BiquadCoeffs {
    let w0 = 2.0 * PI * freq_hz / sample_rate_hz;
    let alpha = (w0 / 2.0).sin() / (2.0 * q);
    let cos_w0 = w0.cos();
    let b0 = (1.0 + cos_w0) * 0.5;
    let b1 = -(1.0 + cos_w0);
    let b2 = b0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;
    normalize(b0, b1, b2, a0, a1, a2)
}
fn bandpass(center_hz: f32, sample_rate_hz: f32, q: f32) -> BiquadCoeffs {
    let w0 = 2.0 * PI * center_hz / sample_rate_hz;
    let alpha = (w0 / 2.0).sin() / (2.0 * q);
    let cos_w0 = w0.cos();
    let sin_w0 = w0.sin();
    let b0 = sin_w0 / 2.0 / q;
    let b1 = 0.0;
    let b2 = -b0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;
    normalize(b0, b1, b2, a0, a1, a2)
}
fn notch(center_hz: f32, sample_rate_hz: f32, q: f32) -> BiquadCoeffs {
    let w0 = 2.0 * PI * center_hz / sample_rate_hz;
    let alpha = (w0 / 2.0).sin() / (2.0 * q);
    let cos_w0 = w0.cos();
    let b0 = 1.0;
    let b1 = -2.0 * cos_w0;
    let b2 = 1.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w0;
    let a2 = 1.0 - alpha;
    normalize(b0, b1, b2, a0, a1, a2)
}
fn normalize(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> BiquadCoeffs {
    let a0_inv = 1.0 / a0;
    BiquadCoeffs {
        b0: b0 * a0_inv,
        b1: b1 * a0_inv,
        b2: b2 * a0_inv,
        a1: a1 * a0_inv,
        a2: a2 * a0_inv,
    }
}
