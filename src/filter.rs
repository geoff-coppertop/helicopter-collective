/// A single-pole exponential moving average (first-order IIR low-pass) filter.
///
/// y\[n\] = alpha * x\[n\] + (1 - alpha) * y\[n-1\]
///
/// `alpha` in (0, 1] trades responsiveness for noise rejection: values near
/// 1.0 track the input closely, values near 0.0 smooth aggressively at the
/// cost of lag.
pub struct Ema {
    alpha: f32,
    state: Option<f32>,
}

impl Ema {
    pub const fn new(alpha: f32) -> Self {
        Self { alpha, state: None }
    }

    /// Feeds one new sample through the filter and returns the updated
    /// output. The first sample seeds the filter state directly, so there's
    /// no startup transient converging from zero.
    pub fn update(&mut self, sample: f32) -> f32 {
        let y = match self.state {
            Some(prev) => prev + self.alpha * (sample - prev),
            None => sample,
        };
        self.state = Some(y);
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
