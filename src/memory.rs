use std::collections::HashSet;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

pub struct MemoryMeter {
    system: System,
    pid: Pid,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn responsibility_get_pid_responsible_for_pid(pid: i32) -> i32;
}

impl MemoryMeter {
    pub fn new() -> Self {
        Self { system: System::new(), pid: Pid::from_u32(std::process::id()) }
    }

    pub fn total_mb(&mut self) -> u64 {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_memory(),
        );
        let owned = self.owned_processes();
        let bytes: u64 = owned.iter().filter_map(|p| self.system.process(*p)).map(|p| p.memory()).sum();
        bytes / (1024 * 1024)
    }

    #[cfg(target_os = "macos")]
    fn owned_processes(&self) -> HashSet<Pid> {
        let responsible = |p: Pid| {
            // SAFETY: takes a plain pid and has no side effects
            unsafe { responsibility_get_pid_responsible_for_pid(p.as_u32() as i32) }
        };
        let target = responsible(self.pid);
        self.system
            .processes()
            .iter()
            .filter(|(p, s)| {
                **p == self.pid
                    || (s.name().to_string_lossy().starts_with("com.apple.WebKit.") && responsible(**p) == target)
            })
            .map(|(p, _)| *p)
            .collect()
    }

    #[cfg(not(target_os = "macos"))]
    fn owned_processes(&self) -> HashSet<Pid> {
        let mut set = HashSet::from([self.pid]);
        loop {
            let before = set.len();
            for (pid, process) in self.system.processes() {
                if process.parent().is_some_and(|parent| set.contains(&parent)) {
                    set.insert(*pid);
                }
            }
            if set.len() == before {
                return set;
            }
        }
    }
}
