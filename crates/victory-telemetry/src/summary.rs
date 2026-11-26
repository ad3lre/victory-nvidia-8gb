//! Local summary maths. The server recomputes the same figures so a client
//! bug can never be mistaken for a real regression.

use crate::json::Json;

/// One line of `samples.jsonl`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    pub t_ms: f64,
    pub fps: f64,
    pub ms: f64,
    pub draws: f64,
    pub cpu_ms: f64,
    pub gpu_ms: f64,
    pub after_present_ms: f64,
    pub until_submit_ms: f64,
    pub watch_ms: f64,
    pub guard_ms: f64,
    pub guard_faults: f64,
    pub cpu_pct: Option<f64>,
    pub gpu_pct: Option<f64>,
    pub ram_mb: Option<f64>,
    pub ram_avail_mb: Option<f64>,
    pub commit_avail_mb: Option<f64>,
    pub page_faults: Option<f64>,
    pub proc_user_ms: Option<f64>,
    pub proc_kernel_ms: Option<f64>,
    pub cpu_temp_c: Option<f64>,
    pub cpu_temp_reason: Option<String>,
    pub gpu_temp_c: Option<f64>,
    pub gpu_temp_reason: Option<String>,
}

impl Sample {
    pub fn to_json(&self) -> Json {
        let opt = |v: Option<f64>| v.map_or(Json::Null, Json::number);
        let reason = |v: &Option<String>| v.as_deref().map_or(Json::Null, Json::string);
        Json::Obj(vec![
            ("t_ms", Json::number(self.t_ms)),
            ("fps", Json::number(self.fps)),
            ("ms", Json::number(self.ms)),
            ("draws", Json::number(self.draws)),
            ("cpu_ms", Json::number(self.cpu_ms)),
            ("gpu_ms", Json::number(self.gpu_ms)),
            ("after_present_ms", Json::number(self.after_present_ms)),
            ("until_submit_ms", Json::number(self.until_submit_ms)),
            ("watch_ms", Json::number(self.watch_ms)),
            ("guard_ms", Json::number(self.guard_ms)),
            ("guard_faults", Json::number(self.guard_faults)),
            ("cpu_pct", opt(self.cpu_pct)),
            ("gpu_pct", opt(self.gpu_pct)),
            ("ram_mb", opt(self.ram_mb)),
            ("ram_avail_mb", opt(self.ram_avail_mb)),
            ("commit_avail_mb", opt(self.commit_avail_mb)),
            ("page_faults", opt(self.page_faults)),
            ("proc_user_ms", opt(self.proc_user_ms)),
            ("proc_kernel_ms", opt(self.proc_kernel_ms)),
            ("cpu_temp_c", opt(self.cpu_temp_c)),
            ("cpu_temp_reason", reason(&self.cpu_temp_reason)),
            ("gpu_temp_c", opt(self.gpu_temp_c)),
            ("gpu_temp_reason", reason(&self.gpu_temp_reason)),
        ])
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub sample_count: usize,
    pub duration_ms: f64,
    pub fps_mean: f64,
    pub fps_min: f64,
    pub fps_max: f64,
    pub frame_ms_p50: f64,
    pub frame_ms_p95: f64,
    pub frame_ms_p99: f64,
    pub p1_low_fps: f64,
    pub longest_stall_ms: f64,
    pub stutters: usize,
}

impl Summary {
    pub fn to_json(&self) -> Json {
        Json::Obj(vec![
            ("sample_count", Json::Int(self.sample_count as u64)),
            ("duration_ms", Json::number(self.duration_ms)),
            ("fps_mean", Json::number(self.fps_mean)),
            ("fps_min", Json::number(self.fps_min)),
            ("fps_max", Json::number(self.fps_max)),
            ("frame_ms_p50", Json::number(self.frame_ms_p50)),
            ("frame_ms_p95", Json::number(self.frame_ms_p95)),
            ("frame_ms_p99", Json::number(self.frame_ms_p99)),
            ("p1_low_fps", Json::number(self.p1_low_fps)),
            ("longest_stall_ms", Json::number(self.longest_stall_ms)),
            ("stutters_gt_1_5x_median", Json::Int(self.stutters as u64)),
        ])
    }
}

fn percentile(ordered: &[f64], fraction: f64) -> f64 {
    if ordered.is_empty() {
        return 0.0;
    }
    let position = (ordered.len() - 1) as f64 * fraction;
    let low = position.floor() as usize;
    let high = position.ceil() as usize;
    if low == high {
        return ordered[low];
    }
    ordered[low] * (high as f64 - position) + ordered[high] * (position - low as f64)
}

pub fn compute(samples: &[Sample]) -> Summary {
    let mut frame_ms: Vec<f64> = Vec::with_capacity(samples.len());
    let mut fps_values: Vec<f64> = Vec::with_capacity(samples.len());
    for sample in samples {
        if sample.fps.is_finite() && sample.fps > 0.0 {
            fps_values.push(sample.fps);
            let ms = if sample.ms.is_finite() && sample.ms > 0.0 {
                sample.ms
            } else {
                1000.0 / sample.fps
            };
            frame_ms.push(ms);
        }
    }
    if frame_ms.is_empty() {
        return Summary {
            sample_count: samples.len(),
            ..Default::default()
        };
    }
    frame_ms.sort_by(f64::total_cmp);
    let p50 = percentile(&frame_ms, 0.5);
    let p95 = percentile(&frame_ms, 0.95);
    let p99 = percentile(&frame_ms, 0.99);
    let stutters = frame_ms
        .iter()
        .filter(|&&ms| p50 > 0.0 && ms > p50 * 1.5)
        .count();
    let duration =
        (samples.last().map_or(0.0, |s| s.t_ms) - samples.first().map_or(0.0, |s| s.t_ms)).max(0.0);
    Summary {
        sample_count: samples.len(),
        duration_ms: duration,
        fps_mean: fps_values.iter().sum::<f64>() / fps_values.len() as f64,
        fps_min: fps_values.iter().copied().fold(f64::INFINITY, f64::min),
        fps_max: fps_values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        frame_ms_p50: p50,
        frame_ms_p95: p95,
        frame_ms_p99: p99,
        p1_low_fps: if p99 > 0.0 { 1000.0 / p99 } else { 0.0 },
        longest_stall_ms: frame_ms[frame_ms.len() - 1],
        stutters,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t: f64, fps: f64, ms: f64) -> Sample {
        Sample {
            t_ms: t,
            fps,
            ms,
            ..Default::default()
        }
    }

    #[test]
    fn empty_input_is_zeroed() {
        assert_eq!(compute(&[]), Summary::default());
    }

    #[test]
    fn counts_stutters_and_reports_p1_low() {
        let summary = compute(&[
            sample(0.0, 60.0, 16.7),
            sample(1000.0, 60.0, 16.7),
            sample(2000.0, 31.25, 32.0),
        ]);
        assert_eq!(summary.sample_count, 3);
        assert!((summary.fps_mean - 50.416).abs() < 0.01);
        assert!(summary.longest_stall_ms >= 31.9);
        assert!(summary.p1_low_fps > 0.0 && summary.p1_low_fps < 60.0);
    }

    #[test]
    fn sample_json_has_null_temps_with_reasons() {
        let value = Sample {
            cpu_temp_reason: Some("sensor unavailable".into()),
            gpu_temp_reason: Some("NVAPI not available".into()),
            ..sample(0.0, 60.0, 16.7)
        }
        .to_json()
        .to_string();
        assert!(value.contains("\"cpu_temp_c\":null"));
        assert!(value.contains("\"cpu_temp_reason\":\"sensor unavailable\""));
        assert!(value.contains("\"gpu_temp_c\":null"));
    }
}
