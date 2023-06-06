use std::{collections::HashMap, time::{Duration, SystemTime, UNIX_EPOCH}, sync::Arc, net::IpAddr};

pub struct SlidingWindow {
    pub window_duration: u64,
    pub rate_limit: u64,
    pub current_window: u64,
    current_counts: Arc<HashMap<IpAddr, u64>>,
    last_counts: Arc<HashMap<IpAddr, u64>>
}

impl SlidingWindow {
    pub fn new(window_duration: u64, rate_limit: u64) -> Self {
        let mut window = SlidingWindow {
            window_duration,
            rate_limit,
            current_window: 0,
            current_counts: Arc::new(HashMap::new()),
            last_counts: Arc::new(HashMap::new())
        };
        window.current_window = window.get_current_window();
        window
    }

    pub fn get_current_window(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards").as_secs() / self.window_duration
    }

    pub fn elapsed_current_window(&self) -> f32 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH + Duration::from_secs(self.current_window * self.window_duration))
            .expect("Time went backwards").as_secs_f32() / self.window_duration as f32
    }

    pub fn update_current_window(&mut self) {
        let current_window = self.get_current_window();
        if current_window != self.current_window {
            self.last_counts = self.current_counts.clone();
            self.current_counts = Arc::new(HashMap::new());
            
            if self.current_window + 1 != current_window {
                self.last_counts = Arc::new(HashMap::new());
            }
            self.current_window = current_window;
        }
    }


    /// Updates current window and current/last ip counts if needed. Returns
    /// whether or not the packet should pass with the rate limiter policy
    pub fn check_ip_passes(&mut self, ip: &IpAddr) -> bool {
        self.update_current_window();
        let current_count_for_ip = *self.current_counts.get(ip).unwrap_or(&0);
        let last_count_for_ip = *self.last_counts.get(ip).unwrap_or(&0);

        let ratio_elapsed = self.elapsed_current_window();
        let count_for_ip = ((current_count_for_ip as f32) * ratio_elapsed) + (
            last_count_for_ip as f32 * (1.0 - ratio_elapsed)
        );

        return count_for_ip < self.rate_limit as f32;
    }

    /// Should only be called if you can ensure the caller has exclusive access 
    /// to SlidingWindow. (Like it has aquired a mutex lock)
    pub fn increment_count(&mut self, ip: IpAddr) {
        Arc::get_mut(&mut self.current_counts).unwrap().entry(ip)
            .and_modify(|c| { *c += 1 })
            .or_insert(1);
    }

    /// Updates rate limiter and returns false if IP should be rate limited,
    /// true otherwise.
    pub fn check_and_increment(&mut self, ip: IpAddr) -> bool {
        let should_pass = self.check_ip_passes(&ip);
        self.increment_count(ip);
        should_pass
    }
}
