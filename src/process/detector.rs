use std::collections::HashMap;
use sysinfo::System;

use crate::config::CommunicatorConfig;

/// Detects whether a communicator (Discord, Teams, ...) is currently running and
/// exposes the process ids of *all* of its processes, so their audio sessions
/// can be found.
pub struct ProcessDetector {
    system: System,
}

impl ProcessDetector {
    pub fn new() -> Self {
        Self {
            system: System::new(),
        }
    }

    /// Refreshes the process snapshot.
    pub fn refresh(&mut self) {
        self.system.refresh_processes();
    }

    /// All process ids whose name matches one of `process_names`.
    pub fn pids_matching(&self, process_names: &[String]) -> Vec<u32> {
        self.system
            .processes()
            .iter()
            .filter(|(_, process)| {
                let name = process.name();
                process_names
                    .iter()
                    .any(|target| name.eq_ignore_ascii_case(target))
            })
            .map(|(pid, _)| pid.as_u32())
            .collect()
    }

    /// Name of a process, if it is present in the current snapshot.
    pub fn process_name(&self, pid: u32) -> Option<String> {
        self.system
            .process(sysinfo::Pid::from_u32(pid))
            .map(|process| process.name().to_string())
    }

    /// Refreshes the snapshot and returns the process ids of every configured
    /// communicator. Communicators with an empty list are not running.
    pub fn communicator_pids(
        &mut self,
        communicators: &[CommunicatorConfig],
    ) -> HashMap<String, Vec<u32>> {
        self.refresh();
        communicators
            .iter()
            .map(|comm| (comm.id.clone(), self.pids_matching(&comm.process_names)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current_process_names() -> Vec<String> {
        let exe = std::env::current_exe().expect("current exe");
        let name = exe
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .to_string();
        // On Linux the process name is limited to 15 characters, so also try
        // the truncated form.
        let truncated: String = name.chars().take(15).collect();
        vec![name, truncated]
    }

    #[test]
    fn detects_the_current_process() {
        let mut detector = ProcessDetector::new();
        detector.refresh();

        let pids = detector.pids_matching(&current_process_names());
        assert!(!pids.is_empty(), "detector did not find its own process");
    }

    #[test]
    fn reports_pids_for_the_current_process() {
        let mut detector = ProcessDetector::new();
        detector.refresh();

        let pids = detector.pids_matching(&current_process_names());
        assert!(!pids.is_empty(), "expected at least the test process itself");
        assert!(pids.contains(&std::process::id()));
    }

    #[test]
    fn unknown_process_is_not_running() {
        let mut detector = ProcessDetector::new();
        detector.refresh();
        assert!(detector
            .pids_matching(&["definitely-not-a-real-process.exe".to_string()])
            .is_empty());
    }
}
