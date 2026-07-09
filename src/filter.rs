use core::f32::consts::PI;

/// Bare single-value exponential low-pass state, parameterized by a
/// per-call alpha rather than one fixed at construction. [`Ema`] and
/// [`OneEuroFilter`] both build on this: `Ema` fixes alpha once,
/// `OneEuroFilter` recomputes it every sample from the signal's estimated
/// speed.
struct LowPassState {
    state: Option<f32>,
}

impl LowPassState {
    const fn new() -> Self {
        Self { state: None }
    }

    /// y\[n\] = alpha * x\[n\] + (1 - alpha) * y\[n-1\]. The first sample
    /// seeds the state directly, so there's no startup transient converging
    /// from zero.
    fn update(&mut self, sample: f32, alpha: f32) -> f32 {
        let y = match self.state {
            Some(prev) => prev + alpha * (sample - prev),
            None => sample,
        };
        self.state = Some(y);
        y
    }
}

/// A single-pole exponential moving average (first-order IIR low-pass) filter.
///
/// `alpha` in (0, 1] trades responsiveness for noise rejection: values near
/// 1.0 track the input closely, values near 0.0 smooth aggressively at the
/// cost of lag — the same knob controls both, so you can't get less lag
/// without also passing more noise. See [`OneEuroFilter`] for a filter that
/// decouples the two by adapting to the signal's speed.
pub struct Ema {
    alpha: f32,
    inner: LowPassState,
}

impl Ema {
    pub const fn new(alpha: f32) -> Self {
        Self {
            alpha,
            inner: LowPassState::new(),
        }
    }

    /// Feeds one new sample through the filter and returns the updated
    /// output.
    pub fn update(&mut self, sample: f32) -> f32 {
        self.inner.update(sample, self.alpha)
    }
}

/// Converts a cutoff frequency (Hz) and fixed sample period (s) into the
/// equivalent single-pole low-pass alpha: `alpha = 1 / (1 + tau/Te)`, where
/// `tau = 1 / (2*pi*cutoff)`.
fn alpha_for_cutoff(cutoff_hz: f32, sample_period_s: f32) -> f32 {
    let tau = 1.0 / (2.0 * PI * cutoff_hz);
    1.0 / (1.0 + tau / sample_period_s)
}

/// A speed-adaptive low-pass filter for noisy, "live" signals — e.g. a
/// magnetometer axis driving pointer-style motion — after Casiez, Roussel &
/// Vogel, "1€ Filter: A Simple Speed-based Low-pass Filter for Noisy Input
/// in Interactive Systems" (CHI 2012).
///
/// Each sample, it estimates the signal's speed (the filtered derivative)
/// and raises the cutoff frequency — lowering the effective smoothing —
/// when the signal is moving fast, while keeping a low cutoff (heavy
/// smoothing) when it's nearly still. This is the mechanism [`Ema`] lacks:
/// one knob (alpha) can't independently tune resting jitter rejection
/// against motion lag, but two speeds having two different needs is exactly
/// the situation a live positional signal is in.
pub struct OneEuroFilter {
    mincutoff: f32,
    beta: f32,
    sample_period_s: f32,
    x: LowPassState,
    dx: Ema,
    prev_sample: Option<f32>,
}

impl OneEuroFilter {
    /// `mincutoff` (Hz) is the cutoff — and so the smoothing — used when the
    /// signal is still; lower means heavier resting smoothing. `beta` scales
    /// how much the cutoff rises per unit of estimated speed; `0.0` disables
    /// adaptation entirely (equivalent to a fixed low-pass at `mincutoff`).
    /// `dcutoff` (Hz) is the fixed cutoff used to smooth the speed estimate
    /// itself — the original paper's usual default is `1.0` and rarely needs
    /// tuning. `sample_period_s` must match the actual, fixed interval
    /// between `update` calls.
    pub fn new(mincutoff: f32, beta: f32, dcutoff: f32, sample_period_s: f32) -> Self {
        Self {
            mincutoff,
            beta,
            sample_period_s,
            x: LowPassState::new(),
            dx: Ema::new(alpha_for_cutoff(dcutoff, sample_period_s)),
            prev_sample: None,
        }
    }

    pub fn update(&mut self, sample: f32) -> f32 {
        let raw_speed = match self.prev_sample {
            Some(prev) => (sample - prev) / self.sample_period_s,
            None => 0.0,
        };
        self.prev_sample = Some(sample);

        let speed = self.dx.update(raw_speed);
        let abs_speed = if speed < 0.0 { -speed } else { speed };
        let cutoff = self.mincutoff + self.beta * abs_speed;
        let alpha = alpha_for_cutoff(cutoff, self.sample_period_s);
        self.x.update(sample, alpha)
    }
}

/// Rounds `value` to the nearest `1 / scale` (e.g. `scale = 1000.0` rounds to
/// 3 decimal places).
///
/// defmt's `{}` format specifier has no `core::fmt`-style precision hint
/// (`{:.N}`), so trimming a noisy float's displayed precision means
/// rounding the value itself before logging it. `f32::round`/`powi` need
/// `std` or `libm`, neither of which this crate depends on, so this rounds
/// half-away-from-zero using only a float-to-int cast (a core language
/// feature, not a libm call).
pub fn round_to(value: f32, scale: f32) -> f32 {
    let scaled = value * scale;
    let rounded = if scaled >= 0.0 {
        (scaled + 0.5) as i32
    } else {
        (scaled - 0.5) as i32
    };
    rounded as f32 / scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_to_truncates_to_requested_precision() {
        assert!((round_to(1.23456, 100.0) - 1.23).abs() < 1e-5);
        assert!((round_to(1.23456, 1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn round_to_rounds_to_nearest() {
        assert!((round_to(1.237, 100.0) - 1.24).abs() < 1e-5);
        assert!((round_to(-1.237, 100.0) - (-1.24)).abs() < 1e-5);
    }

    #[test]
    fn first_sample_is_returned_unfiltered() {
        let mut f = Ema::new(0.2);
        assert_eq!(f.update(10.0), 10.0);
    }

    #[test]
    fn constant_input_converges_to_itself() {
        let mut f = Ema::new(0.5);
        let mut y = 0.0;
        for _ in 0..50 {
            y = f.update(5.0);
        }
        assert!((y - 5.0).abs() < 1e-6);
    }

    #[test]
    fn step_response_moves_toward_new_value_without_overshoot() {
        let mut f = Ema::new(0.3);
        f.update(0.0);
        let y1 = f.update(10.0);
        assert!((y1 - 3.0).abs() < 1e-6);
        assert!(y1 < 10.0);
    }

    #[test]
    fn alpha_one_tracks_input_exactly() {
        let mut f = Ema::new(1.0);
        f.update(1.0);
        assert_eq!(f.update(7.0), 7.0);
    }

    #[test]
    fn one_euro_first_sample_is_returned_unfiltered() {
        let mut f = OneEuroFilter::new(1.0, 0.0, 1.0, 0.1);
        assert_eq!(f.update(5.0), 5.0);
    }

    #[test]
    fn one_euro_with_zero_beta_matches_fixed_cutoff_ema() {
        // beta=0 means the cutoff never adapts away from mincutoff, so this
        // must behave exactly like a fixed-alpha Ema at that cutoff.
        let te = 0.1;
        let mincutoff = 2.0;
        let mut euro = OneEuroFilter::new(mincutoff, 0.0, 1.0, te);
        let mut ema = Ema::new(alpha_for_cutoff(mincutoff, te));
        for x in [1.0, 3.0, 2.0, 5.0, 4.0, 4.0, 4.0] {
            assert!((euro.update(x) - ema.update(x)).abs() < 1e-5);
        }
    }

    #[test]
    fn one_euro_tracks_a_fast_jump_more_closely_than_a_fixed_low_cutoff() {
        let te = 0.1;
        let mincutoff = 0.5;
        let mut euro = OneEuroFilter::new(mincutoff, 5.0, 1.0, te);
        let mut ema = Ema::new(alpha_for_cutoff(mincutoff, te));

        euro.update(0.0);
        ema.update(0.0);

        // A big, fast jump: the speed estimate should push the adaptive
        // filter's effective cutoff well above mincutoff, tracking the new
        // value more closely than the fixed-cutoff filter on this very
        // next sample.
        let euro_y = euro.update(10.0);
        let ema_y = ema.update(10.0);
        assert!(euro_y > ema_y);
    }

    #[test]
    fn one_euro_smooths_as_heavily_as_fixed_low_cutoff_when_nearly_still() {
        let te = 0.1;
        let mincutoff = 0.5;
        let beta = 0.05;
        let mut euro = OneEuroFilter::new(mincutoff, beta, 1.0, te);
        let mut ema = Ema::new(alpha_for_cutoff(mincutoff, te));

        // Small sample-to-sample wobble keeps the estimated speed tiny, so
        // the adaptive cutoff should stay close to mincutoff and the two
        // filters should track each other closely.
        for x in [1.0, 1.01, 0.99, 1.0, 1.02, 0.98] {
            let euro_y = euro.update(x);
            let ema_y = ema.update(x);
            assert!((euro_y - ema_y).abs() < 0.05);
        }
    }
}
