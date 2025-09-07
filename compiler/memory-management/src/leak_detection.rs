//! Memory leak detection utilities

use crate::{MemoryError, arc_runtime::get_arc_stats};
use std::time::Instant;

/// Leak detector for NEURO memory management
pub struct LeakDetector {
    enabled: bool,
    last_check: parking_lot::Mutex<Instant>,
    check_interval: std::time::Duration,
}

impl LeakDetector {
    /// Create a new leak detector
    pub fn new() -> Self {
        Self {
            enabled: cfg!(debug_assertions),
            last_check: parking_lot::Mutex::new(Instant::now()),
            check_interval: std::time::Duration::from_secs(60), // Check every minute
        }
    }

    /// Enable or disable leak detection
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Set the check interval
    pub fn set_check_interval(&mut self, interval: std::time::Duration) {
        self.check_interval = interval;
    }

    /// Check for potential memory leaks
    pub fn check_leaks(&self) -> Result<LeakReport, MemoryError> {
        if !self.enabled {
            return Ok(LeakReport::disabled());
        }

        let mut last_check = self.last_check.lock();
        let now = Instant::now();
        
        if now.duration_since(*last_check) < self.check_interval {
            return Ok(LeakReport::skipped());
        }

        *last_check = now;
        drop(last_check);

        // Check ARC objects
        let arc_stats = get_arc_stats();
        let mut issues = Vec::new();

        // Look for patterns that suggest leaks
        if arc_stats.live_objects > 10000 {
            issues.push(LeakIssue {
                issue_type: LeakIssueType::TooManyLiveObjects,
                description: format!("High number of live objects: {}", arc_stats.live_objects),
                severity: if arc_stats.live_objects > 100000 { 
                    LeakSeverity::Critical 
                } else { 
                    LeakSeverity::Warning 
                },
                object_count: Some(arc_stats.live_objects),
            });
        }

        // Check for dominant object types that might indicate leaks
        for (type_name, count) in &arc_stats.objects_by_type {
            if *count > 1000 {
                issues.push(LeakIssue {
                    issue_type: LeakIssueType::DominantObjectType,
                    description: format!("High count of objects of type '{}': {}", type_name, count),
                    severity: if *count > 10000 { 
                        LeakSeverity::Critical 
                    } else { 
                        LeakSeverity::Warning 
                    },
                    object_count: Some(*count),
                });
            }
        }

        // Estimate memory usage growth
        let estimated_memory = arc_stats.total_memory_estimated;
        if estimated_memory > 1024 * 1024 * 1024 { // 1GB
            issues.push(LeakIssue {
                issue_type: LeakIssueType::HighMemoryUsage,
                description: format!("High estimated memory usage: {} bytes", estimated_memory),
                severity: if estimated_memory > 4 * 1024 * 1024 * 1024 { // 4GB
                    LeakSeverity::Critical
                } else {
                    LeakSeverity::Warning
                },
                object_count: None,
            });
        }

        Ok(LeakReport {
            timestamp: now,
            enabled: true,
            skipped: false,
            issues,
            total_objects: arc_stats.live_objects,
            estimated_memory: estimated_memory,
        })
    }

    /// Get leak detection statistics
    pub fn get_stats(&self) -> LeakDetectorStats {
        LeakDetectorStats {
            enabled: self.enabled,
            check_interval: self.check_interval,
            last_check: *self.last_check.lock(),
        }
    }
}

impl Default for LeakDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Report from leak detection
#[derive(Debug, Clone)]
pub struct LeakReport {
    pub timestamp: Instant,
    pub enabled: bool,
    pub skipped: bool,
    pub issues: Vec<LeakIssue>,
    pub total_objects: usize,
    pub estimated_memory: usize,
}

impl LeakReport {
    fn disabled() -> Self {
        Self {
            timestamp: Instant::now(),
            enabled: false,
            skipped: false,
            issues: Vec::new(),
            total_objects: 0,
            estimated_memory: 0,
        }
    }

    fn skipped() -> Self {
        Self {
            timestamp: Instant::now(),
            enabled: true,
            skipped: true,
            issues: Vec::new(),
            total_objects: 0,
            estimated_memory: 0,
        }
    }

    /// Check if any critical issues were found
    pub fn has_critical_issues(&self) -> bool {
        self.issues.iter().any(|issue| matches!(issue.severity, LeakSeverity::Critical))
    }

    /// Check if any issues were found
    pub fn has_issues(&self) -> bool {
        !self.issues.is_empty()
    }

    /// Get a summary string
    pub fn summary(&self) -> String {
        if !self.enabled {
            return "Leak detection disabled".to_string();
        }

        if self.skipped {
            return "Leak check skipped (too soon since last check)".to_string();
        }

        if self.issues.is_empty() {
            return format!("No leak issues detected ({} objects, ~{} bytes)", 
                          self.total_objects, self.estimated_memory);
        }

        format!("Found {} potential leak issues:\n{}", 
                self.issues.len(),
                self.issues.iter()
                    .map(|issue| format!("  [{}] {}", issue.severity, issue.description))
                    .collect::<Vec<_>>()
                    .join("\n"))
    }
}

/// Individual leak issue
#[derive(Debug, Clone)]
pub struct LeakIssue {
    pub issue_type: LeakIssueType,
    pub description: String,
    pub severity: LeakSeverity,
    pub object_count: Option<usize>,
}

/// Types of leak issues
#[derive(Debug, Clone, PartialEq)]
pub enum LeakIssueType {
    TooManyLiveObjects,
    DominantObjectType,
    HighMemoryUsage,
    SuspiciousGrowthPattern,
}

/// Severity of leak issues
#[derive(Debug, Clone, PartialEq)]
pub enum LeakSeverity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for LeakSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeakSeverity::Info => write!(f, "INFO"),
            LeakSeverity::Warning => write!(f, "WARN"),
            LeakSeverity::Critical => write!(f, "CRIT"),
        }
    }
}

/// Statistics about the leak detector itself
#[derive(Debug, Clone)]
pub struct LeakDetectorStats {
    pub enabled: bool,
    pub check_interval: std::time::Duration,
    pub last_check: Instant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leak_detector_creation() {
        let detector = LeakDetector::new();
        assert_eq!(detector.enabled, cfg!(debug_assertions));
    }

    #[test]
    fn test_leak_detection_disabled() {
        let mut detector = LeakDetector::new();
        detector.set_enabled(false);
        
        let report = detector.check_leaks().unwrap();
        assert!(!report.enabled);
        assert!(!report.has_issues());
    }

    #[test]
    fn test_leak_report_summary() {
        let report = LeakReport {
            timestamp: Instant::now(),
            enabled: true,
            skipped: false,
            issues: vec![
                LeakIssue {
                    issue_type: LeakIssueType::TooManyLiveObjects,
                    description: "Test issue".to_string(),
                    severity: LeakSeverity::Warning,
                    object_count: Some(100),
                }
            ],
            total_objects: 100,
            estimated_memory: 1000,
        };

        let summary = report.summary();
        assert!(summary.contains("1 potential leak issues"));
        assert!(summary.contains("WARN"));
    }

    #[test]
    fn test_check_interval_configuration() {
        let mut detector = LeakDetector::new();
        detector.set_check_interval(std::time::Duration::from_millis(50));
        
        let stats = detector.get_stats();
        assert_eq!(stats.check_interval, std::time::Duration::from_millis(50));
        assert_eq!(stats.enabled, cfg!(debug_assertions));
    }
}