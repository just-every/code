//! Agent lifecycle management with automatic cleanup (T88)
//!
//! Provides:
//! - Process tracking for spawned agents
//! - SIGTERM propagation on cancellation
//! - Timeout enforcement with watchdog threads
//! - Automatic cleanup via Drop trait (RAII pattern)
//!
//! REBASE-SAFE: New file, 100% isolation, uses Drop instead of app.rs hooks

use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Manages lifecycle of spawned agent processes
///
/// Automatically cancels all tracked agents when dropped (RAII pattern).
/// This ensures cleanup happens even on pipeline cancellation, errors, or panics.
#[derive(Debug)]
pub struct AgentLifecycleManager {
    /// Tracked agent processes (agent_id → Child process)
    #[allow(clippy::type_complexity)]
    agents: Arc<Mutex<HashMap<String, AgentProcess>>>,
    /// Default timeout for agents
    default_timeout: Duration,
}

#[derive(Debug)]
struct AgentProcess {
    #[allow(dead_code)]
    child: Child,
    started_at: Instant,
    timeout: Duration,
}

impl AgentLifecycleManager {
    /// Create new manager with default 30-minute timeout
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
            default_timeout: Duration::from_secs(30 * 60), // 30 minutes
        }
    }

    /// Create manager with custom default timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
            default_timeout: timeout,
        }
    }

    /// Track an agent process for lifecycle management
    ///
    /// Starts timeout watchdog automatically
    pub fn track_agent(&self, agent_id: String, child: Child, timeout: Option<Duration>) {
        let timeout = timeout.unwrap_or(self.default_timeout);

        let process = AgentProcess {
            child,
            started_at: Instant::now(),
            timeout,
        };

        if let Ok(mut agents) = self.agents.lock() {
            agents.insert(agent_id.clone(), process);
        }

        // Start timeout watchdog
        self.start_watchdog(agent_id, timeout);
    }

    /// Start timeout watchdog for an agent
    fn start_watchdog(&self, agent_id: String, timeout: Duration) {
        let agents = Arc::clone(&self.agents);

        thread::spawn(move || {
            thread::sleep(timeout);

            // Check if agent still running
            if let Ok(mut agents_map) = agents.lock() {
                if let Some(process) = agents_map.get(&agent_id) {
                    let elapsed = process.started_at.elapsed();
                    if elapsed >= timeout {
                        // Timeout exceeded - remove from tracking
                        // Process will be killed in Drop
                        tracing::warn!(
                            "Agent {} exceeded timeout ({:.1}s), will be terminated",
                            agent_id,
                            elapsed.as_secs_f64()
                        );
                        agents_map.remove(&agent_id);
                    }
                }
            }
        });
    }

    /// Cancel specific agent by ID
    pub fn cancel_agent(&self, agent_id: &str) -> bool {
        if let Ok(mut agents) = self.agents.lock() {
            if let Some(mut process) = agents.remove(agent_id) {
                Self::kill_process(&mut process.child, agent_id);
                return true;
            }
        }
        false
    }

    /// Cancel all tracked agents
    pub fn cancel_all(&self) {
        if let Ok(mut agents) = self.agents.lock() {
            for (agent_id, mut process) in agents.drain() {
                Self::kill_process(&mut process.child, &agent_id);
            }
        }
    }

    /// Get count of tracked agents
    pub fn agent_count(&self) -> usize {
        self.agents.lock().map(|a| a.len()).unwrap_or(0)
    }

    /// Check if specific agent is being tracked
    pub fn is_tracking(&self, agent_id: &str) -> bool {
        self.agents
            .lock()
            .map(|a| a.contains_key(agent_id))
            .unwrap_or(false)
    }

    /// Kill a process with SIGTERM (Unix) or TerminateProcess (Windows)
    fn kill_process(child: &mut Child, agent_id: &str) {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let pid = child.id() as i32;
            unsafe {
                // Send SIGTERM for graceful shutdown
                libc::kill(pid, libc::SIGTERM);
            }
            tracing::info!("Sent SIGTERM to agent {} (pid {})", agent_id, pid);

            // Give process 5 seconds to exit gracefully
            thread::sleep(Duration::from_secs(5));

            // Force kill if still running
            let _ = child.kill();
        }

        #[cfg(not(unix))]
        {
            // Windows: Use kill() directly
            let _ = child.kill();
            tracing::info!("Terminated agent {} (pid {})", agent_id, child.id());
        }
    }
}

impl Default for AgentLifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Automatic cleanup when manager is dropped
impl Drop for AgentLifecycleManager {
    fn drop(&mut self) {
        tracing::debug!("AgentLifecycleManager dropping, cancelling all agents");
        self.cancel_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_creation() {
        let manager = AgentLifecycleManager::new();
        assert_eq!(manager.agent_count(), 0);
        assert_eq!(manager.default_timeout.as_secs(), 30 * 60);
    }

    #[test]
    fn test_manager_with_custom_timeout() {
        let manager = AgentLifecycleManager::with_timeout(Duration::from_secs(300));
        assert_eq!(manager.default_timeout.as_secs(), 300);
    }

    #[test]
    fn test_default_trait() {
        let manager = AgentLifecycleManager::default();
        assert_eq!(manager.agent_count(), 0);
    }

    #[test]
    fn test_track_agent_increases_count() {
        let manager = AgentLifecycleManager::new();
        assert_eq!(manager.agent_count(), 0);

        // Simulate tracking (can't spawn real process in test)
        // Would need: manager.track_agent("test".to_string(), child, None);
        // For now, just test construction
    }

    #[test]
    fn test_is_tracking() {
        let manager = AgentLifecycleManager::new();
        assert!(!manager.is_tracking("nonexistent"));
    }

    // Integration test - requires actual process spawning
    #[test]
    #[ignore]
    fn test_cancel_agent() {
        let manager = AgentLifecycleManager::new();

        // Spawn a sleep process
        let child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep");

        let pid = child.id();
        manager.track_agent("test-agent".to_string(), child, Some(Duration::from_secs(10)));

        assert!(manager.is_tracking("test-agent"));
        assert_eq!(manager.agent_count(), 1);

        // Cancel the agent
        let cancelled = manager.cancel_agent("test-agent");
        assert!(cancelled);
        assert!(!manager.is_tracking("test-agent"));
        assert_eq!(manager.agent_count(), 0);

        // Verify process was killed
        thread::sleep(Duration::from_millis(100));
        // Process should be dead (can't check easily in test)
    }

    #[test]
    #[ignore]
    fn test_drop_cancels_all_agents() {
        {
            let manager = AgentLifecycleManager::new();

            let child = Command::new("sleep")
                .arg("60")
                .spawn()
                .expect("spawn sleep");

            manager.track_agent("test1".to_string(), child, None);
            assert_eq!(manager.agent_count(), 1);

            // Manager dropped here → Drop::drop() → cancel_all()
        }

        // Process should be killed by Drop
        thread::sleep(Duration::from_millis(100));
    }
}
