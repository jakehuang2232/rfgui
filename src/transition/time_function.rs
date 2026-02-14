#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeFunction {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl TimeFunction {
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Self::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - ((-2.0 * t + 2.0).powi(2) * 0.5)
                }
            }
        }
    }
}

pub fn normalized_timeline_progress(
    elapsed_seconds: f32,
    delay_seconds: f32,
    duration_seconds: f32,
) -> Option<f32> {
    if elapsed_seconds < delay_seconds {
        return None;
    }
    if duration_seconds <= f32::EPSILON {
        return Some(1.0);
    }
    Some(((elapsed_seconds - delay_seconds) / duration_seconds).clamp(0.0, 1.0))
}
