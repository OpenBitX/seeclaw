use crate::errors::SeeClawResult;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct StabilityConfig {
    pub max_wait_ms: u64,
    pub check_interval_ms: u64,
    pub stability_threshold: f64,
    pub min_stable_frames: usize,
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            max_wait_ms: 5000,
            check_interval_ms: 200,
            stability_threshold: 0.02,
            min_stable_frames: 3,
        }
    }
}

pub struct VisualStabilityDetector {
    config: StabilityConfig,
    last_frame_hash: Option<u64>,
    stable_frame_count: usize,
}

impl VisualStabilityDetector {
    pub fn new(config: StabilityConfig) -> Self {
        Self {
            config,
            last_frame_hash: None,
            stable_frame_count: 0,
        }
    }

    pub fn with_default() -> Self {
        Self::new(StabilityConfig::default())
    }

    pub fn reset(&mut self) {
        self.last_frame_hash = None;
        self.stable_frame_count = 0;
    }

    pub fn compute_frame_hash(&self, frame: &[u8]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        
        let sample_step = (frame.len() / 1000).max(1);
        for i in (0..frame.len()).step_by(sample_step) {
            frame[i].hash(&mut hasher);
        }
        
        hasher.finish()
    }

    pub fn compute_frame_difference(&self, frame1: &[u8], frame2: &[u8]) -> f64 {
        if frame1.is_empty() || frame2.is_empty() {
            return 1.0;
        }

        let min_len = frame1.len().min(frame2.len());
        let sample_step = (min_len / 1000).max(1);
        
        let mut diff_count = 0;
        let mut total_samples = 0;

        for i in (0..min_len).step_by(sample_step) {
            let diff = (frame1[i] as i32 - frame2[i] as i32).abs();
            if diff > 10 {
                diff_count += 1;
            }
            total_samples += 1;
        }

        if total_samples == 0 {
            return 0.0;
        }

        diff_count as f64 / total_samples as f64
    }

    pub fn is_stable(&mut self, frame: &[u8]) -> bool {
        let current_hash = self.compute_frame_hash(frame);

        if let Some(last_hash) = self.last_frame_hash {
            if current_hash == last_hash {
                self.stable_frame_count += 1;
            } else {
                self.stable_frame_count = 0;
            }
        }

        self.last_frame_hash = Some(current_hash);
        self.stable_frame_count >= self.config.min_stable_frames
    }
}

pub async fn wait_for_visual_stability<F, Fut>(
    capture_frame: F,
    config: StabilityConfig,
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> SeeClawResult<bool>
where
    F: Fn() -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = SeeClawResult<Vec<u8>>> + Send + 'static,
{
    let mut detector = VisualStabilityDetector::new(config.clone());
    let start_time = std::time::Instant::now();

    while start_time.elapsed() < Duration::from_millis(config.max_wait_ms) {
        if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(false);
        }

        let frame = capture_frame().await?;
        
        if detector.is_stable(&frame) {
            tracing::debug!("Visual stability achieved after {:?}", start_time.elapsed());
            return Ok(true);
        }

        tokio::time::sleep(Duration::from_millis(config.check_interval_ms)).await;
    }

    tracing::warn!("Visual stability timeout after {:?}", start_time.elapsed());
    Ok(false)
}

pub async fn wait_for_animation_completion<F, Fut>(
    capture_frame: F,
    config: StabilityConfig,
    stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> SeeClawResult<bool>
where
    F: Fn() -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = SeeClawResult<Vec<u8>>> + Send + 'static,
{
    let _detector = VisualStabilityDetector::new(config.clone());
    let start_time = std::time::Instant::now();
    let mut last_frame: Option<Vec<u8>> = None;

    tokio::time::sleep(Duration::from_millis(300)).await;

    while start_time.elapsed() < Duration::from_millis(config.max_wait_ms) {
        if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(false);
        }

        let current_frame = capture_frame().await?;

        if let Some(ref prev_frame) = last_frame {
            let detector = VisualStabilityDetector::new(config.clone());
            let diff = detector.compute_frame_difference(prev_frame, &current_frame);
            
            tracing::debug!("Frame difference: {:.4}", diff);

            if diff < config.stability_threshold {
                tokio::time::sleep(Duration::from_millis(config.check_interval_ms)).await;
                
                let verify_frame = capture_frame().await?;
                let verify_diff = detector.compute_frame_difference(&current_frame, &verify_frame);
                
                if verify_diff < config.stability_threshold {
                    tracing::debug!("Animation completion confirmed after {:?}", start_time.elapsed());
                    return Ok(true);
                }
            }
        }

        last_frame = Some(current_frame);
        tokio::time::sleep(Duration::from_millis(config.check_interval_ms)).await;
    }

    tracing::warn!("Animation completion timeout after {:?}", start_time.elapsed());
    Ok(false)
}
