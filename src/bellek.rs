use std::collections::HashSet;

use sysinfo::{Pid, ProcessesToUpdate, System};

/// wolf ve ona bağlı webview süreçlerinin toplam bellek ölçümü
pub struct BellekOlcer {
    sistem: System,
    pid: Pid,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    // libSystem'deki sorumlu süreç sorgusu; WebKit süreçleri uygulamanın çocuğu değil
    fn responsibility_get_pid_responsible_for_pid(pid: i32) -> i32;
}

impl BellekOlcer {
    pub fn new() -> Self {
        Self { sistem: System::new(), pid: Pid::from_u32(std::process::id()) }
    }

    /// Toplam MB (RSS toplamı olduğu için paylaşılan sayfalar birkaç kez sayılabilir)
    pub fn toplam_mb(&mut self) -> u64 {
        self.sistem.refresh_processes(ProcessesToUpdate::All, true);
        let ait = self.ait_surecler();
        let bayt: u64 = ait.iter().filter_map(|p| self.sistem.process(*p)).map(|p| p.memory()).sum();
        bayt / (1024 * 1024)
    }

    #[cfg(target_os = "macos")]
    fn ait_surecler(&self) -> HashSet<Pid> {
        let sorumlu = |p: Pid| {
            // SAFETY: yalnızca sayısal pid alan, yan etkisiz sistem çağrısı
            unsafe { responsibility_get_pid_responsible_for_pid(p.as_u32() as i32) }
        };
        // Başka uygulamadan (IDE, terminal) başlatılınca sorumlu süreç biz olmayabiliriz
        let hedef = sorumlu(self.pid);
        self.sistem
            .processes()
            .iter()
            .filter(|(p, s)| {
                **p == self.pid
                    || (s.name().to_string_lossy().starts_with("com.apple.WebKit.") && sorumlu(**p) == hedef)
            })
            .map(|(p, _)| *p)
            .collect()
    }

    /// Windows ve Linux'ta webview süreçleri uygulamanın alt süreci olarak çalışır
    #[cfg(not(target_os = "macos"))]
    fn ait_surecler(&self) -> HashSet<Pid> {
        let mut kume = HashSet::from([self.pid]);
        loop {
            let onceki = kume.len();
            for (pid, s) in self.sistem.processes() {
                if s.parent().is_some_and(|ebeveyn| kume.contains(&ebeveyn)) {
                    kume.insert(*pid);
                }
            }
            if kume.len() == onceki {
                return kume;
            }
        }
    }
}
